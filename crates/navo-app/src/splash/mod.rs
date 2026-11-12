//! The start splash: a window thread that draws Navo's mark and fades it out.

mod art;
#[cfg(any(windows, test))]
mod canvas;
mod timing;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod windows_desktop;

use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use navo_core::placement::PhysicalRect;
use timing::Event;

pub use art::Art;

/// The start splash: a handle to the window thread that draws the art it was given.
pub struct Splash {
    events: Option<Sender<Event>>,
    thread: Option<JoinHandle<()>>,
}

impl Splash {
    /// Starts the splash with `art`; the window thread may fail to start.
    pub fn start_with(art: Art) -> Self {
        let (events, thread) = spawn(art).unzip();
        Self { events, thread }
    }

    /// Starts the splash with Navo's art.
    pub(crate) fn start() -> Self {
        Self::start_with(Art::NAVO)
    }

    /// A splash that never opens a window.
    pub fn none() -> Self {
        Self {
            events: None,
            thread: None,
        }
    }

    /// Closes the splash and waits for its window thread.
    pub fn close(&mut self) {
        self.events = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    /// Puts the splash near `near`.
    pub fn place(&self, near: PhysicalRect) {
        self.send(Event::Near(near));
    }

    /// Returns a callback that reports the splash is shown, when a window may open.
    pub fn on_shown(self) -> Option<Box<dyn FnOnce()>> {
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
fn spawn(art: Art) -> Option<(Sender<Event>, JoinHandle<()>)> {
    let (sender, events) = std::sync::mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("navo-splash".to_owned())
        .spawn(move || windows::run(&events, art))
        .ok()?;
    Some((sender, thread))
}

#[cfg(not(windows))]
fn spawn(_art: Art) -> Option<(Sender<Event>, JoinHandle<()>)> {
    None
}

#[cfg(test)]
mod tests;
