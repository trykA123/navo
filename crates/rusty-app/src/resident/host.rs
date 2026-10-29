use std::cell::{Cell, RefCell};

use rusty_ui::settings::Values;
use rusty_ui::window_view::{OpenRequest, ResidentHooks};

use super::events::{ResidentConfig, ResidentEvent};

pub(crate) trait Running {
    fn key_taken(&self) -> bool;
    fn has_tray(&self) -> bool;
    fn stop(&mut self) -> bool;
}

pub(crate) struct Host<R> {
    config: Cell<ResidentConfig>,
    running: RefCell<Option<R>>,
    start: Box<dyn Fn(ResidentConfig) -> R>,
}

impl<R: Running> Host<R> {
    pub(crate) fn new(values: &Values, start: impl Fn(ResidentConfig) -> R + 'static) -> Self {
        let config = ResidentConfig::from_values(values);
        let running = start(config);
        Self {
            config: Cell::new(config),
            running: RefCell::new(Some(running)),
            start: Box::new(start),
        }
    }

    pub(crate) fn stop(&self) {
        drop(self.running.borrow_mut().take());
    }
}

impl<R: Running> ResidentHooks for Host<R> {
    fn apply(&self, values: &Values) {
        let config = ResidentConfig::from_values(values);
        let previous = self.config.replace(config);
        if previous == config {
            return;
        }
        let mut running = self.running.borrow_mut();
        if let Some(old) = running.as_mut()
            && !old.stop()
        {
            tracing::warn!("the old tray and hotkey resident did not stop; keeping it");
            self.config.set(previous);
            return;
        }
        drop(running.take());
        *running = Some((self.start)(config));
    }

    fn key_taken(&self) -> bool {
        self.running.borrow().as_ref().is_some_and(R::key_taken)
    }

    fn has_tray(&self) -> bool {
        self.running.borrow().as_ref().is_some_and(R::has_tray)
    }
}

pub(crate) fn request_for(event: ResidentEvent) -> OpenRequest {
    match event {
        ResidentEvent::Summon(at) => OpenRequest {
            summoned_at: Some(at),
            ..OpenRequest::default()
        },
        ResidentEvent::Quit => OpenRequest {
            quit: true,
            ..OpenRequest::default()
        },
    }
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
