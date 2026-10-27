mod icon;
mod invoke;
mod loaded;
mod read;
mod selection;
mod sta;
mod worker;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Instant;

use rusty_core::shell_menu::Submenu;
use windows::Win32::Foundation::HWND;

use super::fill::{FillAsk, Fills};
use super::replaced::Replaced;
use super::station::Health;
use super::{
    InvokeOptions, MenuError, MenuSession, MenuSource, Pending, ScreenPoint, WindowHandle,
    WorkerHealth,
};
use worker::Request;

fn shell_error(context: &'static str) -> impl Fn(windows::core::Error) -> MenuError {
    move |error| MenuError::Shell {
        context,
        message: error.message(),
    }
}

fn hwnd_of(handle: WindowHandle) -> HWND {
    HWND(handle.0 as *mut _)
}

#[derive(Clone)]
struct Link {
    sender: Sender<Request>,
    window: isize,
    retired: Arc<AtomicBool>,
    health: Health,
}

impl Link {
    fn send(&self, request: Request) -> bool {
        self.sender.send(request).is_ok() && sta::wake(self.window)
    }

    fn post(&self, request: Request) -> bool {
        if self.retired.load(Ordering::Acquire) {
            return false;
        }
        self.health.note_waiting(Instant::now());
        self.send(request)
    }

    fn ask<T: Send + 'static>(
        &self,
        make: impl FnOnce(super::pending::Reply<T>) -> Request,
    ) -> Pending<T> {
        let (reply, pending) = Pending::channel();
        if self.post(make(reply)) {
            pending
        } else {
            Pending::ready(Err(MenuError::Stopped))
        }
    }
}

struct Worker {
    link: Link,
    thread: JoinHandle<()>,
}

impl Worker {
    fn spawn(warms: bool) -> Result<Self, MenuError> {
        worker::spawn(warms).map(|(link, thread)| Self { link, thread })
    }

    fn is_hung(&self) -> bool {
        self.link.health.is_hung(Instant::now())
    }
}

struct Workers {
    current: Worker,
    replaced: Replaced,
}

pub(super) struct Service {
    workers: Mutex<Workers>,
}

impl Service {
    pub(super) fn start() -> Result<Self, MenuError> {
        Worker::spawn(true).map(|current| Self {
            workers: Mutex::new(Workers {
                current,
                replaced: Replaced::default(),
            }),
        })
    }

    fn workers(&self) -> std::sync::MutexGuard<'_, Workers> {
        self.workers.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn live_link(&self) -> Link {
        let mut workers = self.workers();
        if workers.current.is_hung()
            && let Ok(fresh) = Worker::spawn(false)
        {
            let stale = std::mem::replace(&mut workers.current, fresh);
            stale.link.retired.store(true, Ordering::Release);
            stale.link.send(Request::Shutdown);
            workers.replaced.add(stale.thread);
        }
        workers.current.link.clone()
    }

    pub(super) fn health(&self) -> WorkerHealth {
        self.workers().replaced.health()
    }

    pub(super) fn classic_for(
        &self,
        source: MenuSource,
        hwnd: WindowHandle,
        point: ScreenPoint,
    ) -> Pending<Option<u32>> {
        self.live_link().ask(|reply| Request::ClassicFor {
            source,
            hwnd,
            point,
            reply,
        })
    }

    pub(super) fn open(&self, source: MenuSource) -> Pending<MenuSession> {
        let link = self.live_link();
        let (reply, cancel, pending) = Pending::cancellable();
        let request = Request::Open {
            source,
            reply,
            cancel,
        };
        if link.post(request) {
            pending
        } else {
            Pending::ready(Err(MenuError::Stopped))
        }
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.workers().current.link.send(Request::Shutdown);
    }
}

pub(super) struct SessionHandle {
    id: u64,
    link: Link,
    fills: Fills,
}

impl SessionHandle {
    pub(super) fn fill(&self, path: Vec<usize>) -> Pending<Submenu> {
        self.fills.ask(path, |ask| self.send_fill(ask))
    }

    pub(super) fn prefill(&self, path: Vec<usize>) -> Pending<Submenu> {
        self.fills.prefill(path, |ask| self.send_fill(ask))
    }

    fn send_fill(&self, ask: FillAsk) {
        let session = self.id;
        self.link.post(match ask {
            FillAsk::Now(path, reply) => Request::Fill {
                session,
                path,
                reply,
            },
            FillAsk::Later(path, reply) => Request::Prefill {
                session,
                path,
                reply,
            },
            FillAsk::Promote(path) => Request::Promote { session, path },
        });
    }

    pub(super) fn invoke(
        &self,
        command: u32,
        verb: Option<String>,
        options: InvokeOptions,
    ) -> Pending<()> {
        let session = self.id;
        self.link.ask(|reply| Request::Invoke {
            session,
            command,
            verb,
            options,
            reply,
        })
    }

    pub(super) fn classic(&self, hwnd: WindowHandle, point: ScreenPoint) -> Pending<Option<u32>> {
        let session = self.id;
        self.link.ask(|reply| Request::Classic {
            session,
            hwnd,
            point,
            reply,
        })
    }
}

impl Drop for SessionHandle {
    fn drop(&mut self) {
        self.link.send(Request::Close { session: self.id });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, mpsc};

    use super::{Health, InvokeOptions, Link, MenuError, Request};

    #[test]
    fn a_retired_link_answers_stopped_without_sending() {
        let (sender, receiver) = mpsc::channel();
        let link = Link {
            sender,
            window: 0,
            retired: Arc::new(AtomicBool::new(true)),
            health: Health::default(),
        };
        let pending = link.ask(|reply| Request::Invoke {
            session: 1,
            command: 1,
            verb: None,
            options: InvokeOptions::default(),
            reply,
        });
        assert_eq!(pending.wait(), Err(MenuError::Stopped));
        assert!(receiver.try_recv().is_err());
    }
}
