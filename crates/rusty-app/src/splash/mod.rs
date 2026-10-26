#[cfg(any(windows, test))]
mod canvas;
mod timing;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod windows_desktop;

use std::sync::mpsc::Sender;

use rusty_core::placement::PhysicalRect;
use timing::Event;

pub(crate) struct Splash(Option<Sender<Event>>);

impl Splash {
    pub(crate) fn start() -> Self {
        Self(spawn())
    }

    pub(crate) fn none() -> Self {
        Self(None)
    }

    pub(crate) fn place(&self, near: PhysicalRect) {
        self.send(Event::Near(near));
    }

    pub(crate) fn on_shown(self) -> Option<Box<dyn FnOnce()>> {
        let sender = self.0?;
        Some(Box::new(move || {
            let _ = sender.send(Event::Ready);
        }))
    }

    fn send(&self, event: Event) {
        if let Some(sender) = &self.0 {
            let _ = sender.send(event);
        }
    }
}

#[cfg(windows)]
fn spawn() -> Option<Sender<Event>> {
    let (sender, events) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("ferrite-splash".to_owned())
        .spawn(move || windows::run(&events))
        .ok()?;
    Some(sender)
}

#[cfg(not(windows))]
fn spawn() -> Option<Sender<Event>> {
    None
}

#[cfg(test)]
mod tests;
