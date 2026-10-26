#[cfg(any(windows, test))]
mod canvas;
mod timing;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod windows_desktop;

use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use rusty_core::placement::PhysicalRect;
use timing::Event;

pub(crate) struct Splash {
    events: Option<Sender<Event>>,
    thread: Option<JoinHandle<()>>,
}

impl Splash {
    pub(crate) fn start() -> Self {
        let (events, thread) = spawn().unzip();
        Self { events, thread }
    }

    pub(crate) fn none() -> Self {
        Self {
            events: None,
            thread: None,
        }
    }

    pub(crate) fn close(&mut self) {
        self.events = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    pub(crate) fn place(&self, near: PhysicalRect) {
        self.send(Event::Near(near));
    }

    pub(crate) fn on_shown(self) -> Option<Box<dyn FnOnce()>> {
        let sender = self.events?;
        Some(Box::new(move || {
            let _ = sender.send(Event::Ready);
        }))
    }

    fn send(&self, event: Event) {
        if let Some(sender) = &self.events {
            let _ = sender.send(event);
        }
    }
}

#[cfg(windows)]
fn spawn() -> Option<(Sender<Event>, JoinHandle<()>)> {
    let (sender, events) = std::sync::mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("navo-splash".to_owned())
        .spawn(move || windows::run(&events))
        .ok()?;
    Some((sender, thread))
}

#[cfg(not(windows))]
fn spawn() -> Option<(Sender<Event>, JoinHandle<()>)> {
    None
}

#[cfg(test)]
mod tests;
