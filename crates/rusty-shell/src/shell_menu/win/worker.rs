use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use rusty_core::shell_menu::Submenu;
use windows::Win32::Foundation::HWND;

use super::invoke::{self, Target};
use super::loaded::LoadedMenu;
use super::sta::{self, Ole, Window};
use super::{Link, SessionHandle};
use crate::shell_menu::fill::{self, Background, FillReply, Fills};
use crate::shell_menu::pending::{Cancel, Reply, answer_unless_cancelled};
use crate::shell_menu::station::{self, BusySince, Station};
use crate::shell_menu::{InvokeOptions, MenuError, MenuSession, ScreenPoint, WindowHandle};

pub(super) enum Request {
    Open {
        paths: Vec<PathBuf>,
        reply: Reply<MenuSession>,
        cancel: Cancel,
    },
    Fill {
        session: u64,
        path: Vec<usize>,
        reply: FillReply,
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
    Close {
        session: u64,
    },
    Shutdown,
}

impl Request {
    fn can_hang(&self) -> bool {
        !matches!(self, Self::Invoke { .. } | Self::Classic { .. })
    }
}

pub(super) fn spawn(busy: BusySince) -> Result<Link, MenuError> {
    let (sender, receiver) = mpsc::channel();
    let (ready_sender, ready) = mpsc::channel();
    std::thread::Builder::new()
        .name("shell-menu".to_owned())
        .spawn(move || run(&receiver, sender, &busy, &ready_sender))
        .map_err(|error| MenuError::Shell {
            context: "starting the shell menu thread",
            message: error.to_string(),
        })?;
    ready.recv().unwrap_or(Err(MenuError::Stopped))
}

fn run(
    receiver: &Receiver<Request>,
    sender: Sender<Request>,
    busy: &BusySince,
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
    };
    let _ = ready.send(Ok(link.clone()));
    let mut worker = Worker {
        link,
        sessions: HashMap::new(),
        next_session: 1,
        background: Vec::new(),
    };
    station::mark(busy, true);
    worker.warm();
    station::mark(busy, false);
    station::serve(&mut worker, receiver, busy);
}

struct Worker {
    link: Link,
    sessions: HashMap<u64, LoadedMenu>,
    next_session: u64,
    background: Vec<Background>,
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
}

impl Worker {
    fn warm(&self) {
        let _ = LoadedMenu::query(&[std::env::temp_dir()], HWND::default());
    }

    fn serve_request(&mut self, request: Request) -> bool {
        match request {
            Request::Open {
                paths,
                reply,
                cancel,
            } => answer_unless_cancelled(&cancel, &reply, || self.open(&paths)),
            Request::Fill {
                session,
                path,
                reply,
            } => self.fill(session, path, reply),
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
                let result = self
                    .session(session)
                    .and_then(|menu| invoke::invoke(menu, &target, options));
                let _ = reply.send(result);
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
            Request::Close { session } => {
                self.sessions.remove(&session);
            }
            Request::Shutdown => return false,
        }
        true
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

    fn open(&mut self, paths: &[PathBuf]) -> Result<MenuSession, MenuError> {
        let mut loaded = LoadedMenu::query(paths, HWND::default())?;
        let menu = loaded.walk();
        let id = self.next_session;
        self.next_session += 1;
        self.sessions.insert(id, loaded);
        Ok(MenuSession {
            menu,
            handle: SessionHandle {
                id,
                link: self.link.clone(),
                fills: Fills::default(),
            },
        })
    }

    fn session(&mut self, id: u64) -> Result<&mut LoadedMenu, MenuError> {
        self.sessions.get_mut(&id).ok_or(MenuError::Closed)
    }
}
