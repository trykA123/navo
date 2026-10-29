mod events;
mod host;
#[cfg(windows)]
mod hotkey;
#[cfg(any(windows, test))]
mod keys;
#[cfg(windows)]
mod thread;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod window;

use std::rc::Rc;
use std::sync::Arc;

use rusty_ui::settings::Values;
use rusty_ui::window_view::{OpenSender, ResidentHooks, WindowShown};

#[cfg(not(windows))]
use events::ResidentConfig;
use events::Sink;
use host::{Host, Running, request_for};
#[cfg(windows)]
use thread::Resident;

pub(crate) struct ResidentHandle(Rc<Host<Resident>>);

impl ResidentHandle {
    pub(crate) fn start(sender: OpenSender, values: &Values) -> Self {
        let sink: Sink = Arc::new(move |event| {
            if !sender.send(request_for(event)) {
                tracing::warn!("the window no longer takes tray and summon requests");
            }
        });
        Self(Rc::new(Host::new(values, move |config| {
            Resident::start(config, sink.clone())
        })))
    }

    pub(crate) fn hooks(&self) -> Rc<dyn ResidentHooks> {
        self.0.clone()
    }

    pub(crate) fn stop(&self) {
        self.0.stop();
    }
}

#[cfg(not(windows))]
struct Resident;

#[cfg(not(windows))]
impl Resident {
    fn start(_config: ResidentConfig, _sink: Sink) -> Self {
        Self
    }

    fn key_taken(&self) -> bool {
        false
    }

    fn has_tray(&self) -> bool {
        false
    }

    fn stop(&mut self) -> bool {
        true
    }
}

impl Running for Resident {
    fn key_taken(&self) -> bool {
        Resident::key_taken(self)
    }

    fn has_tray(&self) -> bool {
        Resident::has_tray(self)
    }

    fn stop(&mut self) -> bool {
        Resident::stop(self)
    }
}

#[cfg(windows)]
pub(crate) fn window_shown() -> Option<WindowShown> {
    Some(std::sync::Arc::new(window::set_shown))
}

#[cfg(not(windows))]
pub(crate) fn window_shown() -> Option<WindowShown> {
    None
}

#[cfg(test)]
mod tests {
    use super::window_shown;

    #[test]
    fn only_windows_gets_the_hide_hook() {
        assert_eq!(window_shown().is_some(), cfg!(windows));
    }

    #[test]
    fn a_window_without_a_handle_is_neither_hidden_nor_shown() {
        if let Some(hook) = window_shown() {
            assert!(!hook(None, false));
            assert!(!hook(None, true));
        }
    }
}
