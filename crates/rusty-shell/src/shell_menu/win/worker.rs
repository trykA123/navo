use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rusty_core::shell_menu::Submenu;
use windows::Win32::Foundation::HWND;

use super::invoke::{self, Target};
use super::loaded::LoadedMenu;
use super::sta::{self, Ole, Window};
use super::{Link, SessionHandle};
use crate::shell_menu::fill::{self, Background, FillReply, Fills};
use crate::shell_menu::idle::{self, Idle, Job, Prefill};
use crate::shell_menu::pending::{Cancel, Reply, answer_unless_cancelled};
use crate::shell_menu::station::{self, Health, Station};
use crate::shell_menu::warm::{self, Warm};
use crate::shell_menu::{
    InvokeOptions, MenuError, MenuSession, MenuSource, ScreenPoint, WindowHandle,
};

pub(super) enum Request {
    Open {
        source: MenuSource,
        reply: Reply<MenuSession>,
        cancel: Cancel,
    },
    Fill {
        session: u64,
        path: Vec<usize>,
        reply: FillReply,
    },
    Prefill {
        session: u64,
        path: Vec<usize>,
        reply: FillReply,
    },
    Promote {
        session: u64,
        path: Vec<usize>,
    },
    Invoke {
        session: u64,
        command: u32,
        verb: Option<String>,
        options: InvokeOptions,
        reply: Reply<()>,
    },
    Classic {
        session: u64,
        hwnd: WindowHandle,
        point: ScreenPoint,
        reply: Reply<Option<u32>>,
    },
    ClassicFor {
        source: MenuSource,
        hwnd: WindowHandle,
        point: ScreenPoint,
        reply: Reply<Option<u32>>,
    },
    Close {
        session: u64,
    },
    Shutdown,
}

impl Request {
    fn can_hang(&self) -> bool {
        !matches!(
            self,
            Self::Invoke { .. } | Self::Classic { .. } | Self::ClassicFor { .. }
        )
    }
}

/// `warms` loads shell extensions while idle; a worker that replaces a hung one skips it.
pub(super) fn spawn(warms: bool) -> Result<(Link, JoinHandle<()>), MenuError> {
    let (sender, receiver) = mpsc::channel();
    let (ready_sender, ready) = mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("shell-menu".to_owned())
        .spawn(move || run(&receiver, (sender, warms), &ready_sender))
        .map_err(|error| MenuError::Shell {
            context: "starting the shell menu thread",
            message: error.to_string(),
        })?;
    let link = ready.recv().unwrap_or(Err(MenuError::Stopped))?;
    Ok((link, thread))
}

fn run(
    receiver: &Receiver<Request>,
    (sender, warms): (Sender<Request>, bool),
    ready: &Sender<Result<Link, MenuError>>,
) {
    let started = Ole::init().and_then(|ole| Window::message_only().map(|window| (ole, window)));
    let (_ole, window) = match started {
        Ok(started) => started,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let link = Link {
        sender,
        window: window.hwnd().0 as isize,
        retired: Arc::default(),
        health: Health::default(),
    };
    let health = link.health.clone();
    let _ = ready.send(Ok(link.clone()));
    let mut worker = Worker {
        link,
        sessions: HashMap::new(),
        next_session: 1,
        background: Vec::new(),
        idle: if warms {
            Idle::warming(warm::first_steps(&std::env::temp_dir()))
        } else {
            Idle::default()
        },
    };
    station::serve(&mut worker, receiver, &health);
}

struct Worker {
    link: Link,
    sessions: HashMap<u64, LoadedMenu>,
    next_session: u64,
    background: Vec<Background>,
    idle: Idle,
}

impl Station for Worker {
    type Request = Request;

    fn can_hang(request: &Request) -> bool {
        request.can_hang()
    }

    fn handle(&mut self, request: Request) -> bool {
        self.serve_request(request)
    }

    fn settle_fills(&mut self) -> Option<Duration> {
        let sessions = &mut self.sessions;
        fill::settle(
            &mut self.background,
            Instant::now(),
            |session, path| match sessions.get_mut(&session) {
                Some(menu) => menu.poll_fill(path),
                None => Some(Err(MenuError::Closed)),
            },
        )
    }

    fn pump(&mut self, timeout: Option<Duration>) -> bool {
        match timeout {
            Some(timeout) => {
                sta::pump_until(Instant::now() + timeout);
                true
            }
            None => sta::wait_message(),
        }
    }

    fn run_idle(&mut self) -> bool {
        match self.idle.next() {
            Some(Job::Prefill(prefill)) => self.run_prefill(prefill),
            Some(Job::Warm(step)) => self.warm(step),
            None => return false,
        }
        true
    }
}

impl Worker {
    fn warm(&mut self, step: Warm) {
        match step {
            Warm::Open { source, scratch } => {
                let Ok(mut loaded) = LoadedMenu::query(&source, HWND::default()) else {
                    return;
                };
                let menu = loaded.walk();
                let session = self.keep(loaded);
                for path in idle::lazy_tops(&menu) {
                    self.idle.warm_later(Warm::Fill { session, path });
                }
                self.idle.warm_later(Warm::Close { session, scratch });
            }
            Warm::Fill { session, path } => self.fill(session, path, FillReply::unread()),
            Warm::Close { session, scratch } => {
                self.sessions.remove(&session);
                drop(scratch);
            }
        }
    }

    fn run_prefill(&mut self, prefill: Prefill) {
        self.fill(prefill.session, prefill.path, prefill.reply);
    }

    fn serve_request(&mut self, request: Request) -> bool {
        match request {
            Request::Open {
                source,
                reply,
                cancel,
            } => answer_unless_cancelled(&cancel, &reply, || self.open(&source)),
            Request::Fill {
                session,
                path,
                reply,
            } => self.fill(session, path, reply),
            Request::Prefill {
                session,
                path,
                reply,
            } => self.idle.prefill(Prefill {
                session,
                path,
                reply,
            }),
            Request::Promote { session, path } => {
                if let Some(prefill) = self.idle.promote(session, &path) {
                    self.run_prefill(prefill);
                }
            }
            Request::Invoke {
                session,
                command,
                verb,
                options,
                reply,
            } => {
                let target = Target {
                    command,
                    verb: verb.as_deref(),
                };
                let _ = reply.send(self.invoke(session, &target, options));
            }
            Request::Classic {
                session,
                hwnd,
                point,
                reply,
            } => {
                let result = self
                    .session(session)
                    .and_then(|menu| invoke::classic(menu, hwnd, point));
                let _ = reply.send(result);
            }
            Request::ClassicFor {
                source,
                hwnd,
                point,
                reply,
            } => {
                let _ = reply.send(classic_for(&source, hwnd, point));
            }
            Request::Close { session } => {
                self.sessions.remove(&session);
                self.idle.close(session);
            }
            Request::Shutdown => return false,
        }
        true
    }

    fn invoke(
        &mut self,
        session: u64,
        target: &Target<'_>,
        options: InvokeOptions,
    ) -> Result<(), MenuError> {
        self.session(session)
            .and_then(|menu| invoke::invoke(menu, target, options))
    }

    fn fill(&mut self, session: u64, path: Vec<usize>, reply: FillReply) {
        match self.session(session).and_then(|menu| menu.fill(&path)) {
            Ok(Submenu::Loading) => {
                let fill = Background::new(session, path, reply, Instant::now());
                self.background.push(fill);
            }
            result => reply.send(result),
        }
    }

    fn open(&mut self, source: &MenuSource) -> Result<MenuSession, MenuError> {
        let mut loaded = LoadedMenu::query(source, HWND::default())?;
        let menu = loaded.walk();
        let id = self.keep(loaded);
        Ok(MenuSession {
            menu,
            handle: SessionHandle {
                id,
                link: self.link.clone(),
                fills: Fills::default(),
            },
        })
    }

    fn keep(&mut self, loaded: LoadedMenu) -> u64 {
        let id = self.next_session;
        self.next_session += 1;
        self.sessions.insert(id, loaded);
        id
    }

    fn session(&mut self, id: u64) -> Result<&mut LoadedMenu, MenuError> {
        self.sessions.get_mut(&id).ok_or(MenuError::Closed)
    }
}

fn classic_for(
    source: &MenuSource,
    hwnd: WindowHandle,
    point: ScreenPoint,
) -> Result<Option<u32>, MenuError> {
    LoadedMenu::query(source, HWND::default()).and_then(|menu| invoke::classic(&menu, hwnd, point))
}
