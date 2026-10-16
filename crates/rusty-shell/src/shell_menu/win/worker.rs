use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use rusty_core::shell_menu::Submenu;
use windows::Win32::Foundation::HWND;

use super::invoke::{self, Target};
use super::loaded::LoadedMenu;
use super::sta::{self, Ole, Window};
use super::{Link, SessionHandle};
use crate::shell_menu::pending::Reply;
use crate::shell_menu::{InvokeOptions, MenuError, MenuSession, ScreenPoint, WindowHandle};

pub(super) enum Request {
    Open {
        paths: Vec<PathBuf>,
        reply: Reply<MenuSession>,
    },
    Fill {
        session: u64,
        path: Vec<usize>,
        reply: Reply<Submenu>,
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

pub(super) type BusySince = Arc<Mutex<Option<Instant>>>;

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
    };
    let _ = ready.send(Ok(link.clone()));
    let mut worker = Worker {
        link,
        owner: window.hwnd(),
        sessions: HashMap::new(),
        next_session: 1,
    };
    mark(busy, true);
    worker.warm();
    mark(busy, false);
    loop {
        while let Ok(request) = receiver.try_recv() {
            mark(busy, true);
            let keep_running = worker.handle(request);
            mark(busy, false);
            if !keep_running {
                return;
            }
        }
        if !sta::wait_message() {
            return;
        }
    }
}

fn mark(busy: &BusySince, is_busy: bool) {
    *busy.lock().unwrap_or_else(PoisonError::into_inner) = is_busy.then(Instant::now);
}

struct Worker {
    link: Link,
    owner: HWND,
    sessions: HashMap<u64, LoadedMenu>,
    next_session: u64,
}

impl Worker {
    fn warm(&self) {
        let _ = LoadedMenu::query(&[std::env::temp_dir()], self.owner);
    }

    fn handle(&mut self, request: Request) -> bool {
        match request {
            Request::Open { paths, reply } => {
                let _ = reply.send(self.open(&paths));
            }
            Request::Fill {
                session,
                path,
                reply,
            } => {
                let _ = reply.send(self.session(session).and_then(|menu| menu.fill(&path)));
            }
            Request::Invoke {
                session,
                command,
                verb,
                options,
                reply,
            } => {
                let owner = self.owner;
                let target = Target {
                    command,
                    verb: verb.as_deref(),
                };
                let result = self
                    .session(session)
                    .and_then(|menu| invoke::invoke(menu, &target, options, owner));
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

    fn open(&mut self, paths: &[PathBuf]) -> Result<MenuSession, MenuError> {
        let mut loaded = LoadedMenu::query(paths, self.owner)?;
        let menu = loaded.walk();
        let id = self.next_session;
        self.next_session += 1;
        self.sessions.insert(id, loaded);
        Ok(MenuSession {
            menu,
            handle: SessionHandle {
                id,
                link: self.link.clone(),
            },
        })
    }

    fn session(&mut self, id: u64) -> Result<&mut LoadedMenu, MenuError> {
        self.sessions.get_mut(&id).ok_or(MenuError::Closed)
    }
}
