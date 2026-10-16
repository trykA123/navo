mod icon;
mod invoke;
mod loaded;
mod read;
mod selection;
mod sta;
mod worker;

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use rusty_core::shell_menu::Submenu;
use windows::Win32::Foundation::HWND;

use super::{InvokeOptions, MenuError, MenuSession, Pending, ScreenPoint, WindowHandle};
use worker::{BusySince, Request};

const HUNG_AFTER: Duration = Duration::from_secs(2);

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
}

impl Link {
    fn send(&self, request: Request) -> bool {
        self.sender.send(request).is_ok() && sta::wake(self.window)
    }

    fn ask<T>(&self, make: impl FnOnce(super::pending::Reply<T>) -> Request) -> Pending<T> {
        let (reply, pending) = Pending::channel();
        if self.send(make(reply)) {
            pending
        } else {
            Pending::ready(Err(MenuError::Stopped))
        }
    }
}

struct Worker {
    link: Link,
    busy: BusySince,
}

impl Worker {
    fn spawn() -> Result<Self, MenuError> {
        let busy = BusySince::default();
        worker::spawn(busy.clone()).map(|link| Self { link, busy })
    }

    fn is_hung(&self) -> bool {
        let busy = self.busy.lock().unwrap_or_else(PoisonError::into_inner);
        busy.is_some_and(|since| since.elapsed() > HUNG_AFTER)
    }
}

pub(super) struct Service {
    worker: Mutex<Worker>,
}

impl Service {
    pub(super) fn start() -> Result<Self, MenuError> {
        Worker::spawn().map(|worker| Self {
            worker: Mutex::new(worker),
        })
    }

    pub(super) fn open(&self, paths: Vec<PathBuf>) -> Pending<MenuSession> {
        let mut worker = self.worker.lock().unwrap_or_else(PoisonError::into_inner);
        if worker.is_hung()
            && let Ok(fresh) = Worker::spawn()
        {
            let stale = std::mem::replace(&mut *worker, fresh);
            stale.link.send(Request::Shutdown);
        }
        worker.link.ask(|reply| Request::Open { paths, reply })
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        let worker = self.worker.lock().unwrap_or_else(PoisonError::into_inner);
        worker.link.send(Request::Shutdown);
    }
}

pub(super) struct SessionHandle {
    id: u64,
    link: Link,
}

impl SessionHandle {
    pub(super) fn fill(&self, path: Vec<usize>) -> Pending<Submenu> {
        let session = self.id;
        self.link.ask(|reply| Request::Fill {
            session,
            path,
            reply,
        })
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
