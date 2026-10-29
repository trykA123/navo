use std::time::{Duration, Instant};

use gpui_kit::{AnyWindowHandle, Keystroke, KeystrokeEvent, Subscription};

use super::FerriteWindow;
use crate::prelude::*;

const HOLD_FOR: Duration = Duration::from_secs(2);
const MAX_HELD: usize = 64;

pub(super) struct FirstKeys {
    window: AnyWindowHandle,
    held: Vec<Keystroke>,
    pub(super) until: Option<Instant>,
}

impl FirstKeys {
    pub(super) fn new(window: &Window) -> Self {
        Self {
            window: window.window_handle(),
            held: Vec::new(),
            until: Some(Instant::now() + HOLD_FOR),
        }
    }

    fn hold(&mut self, keystroke: &Keystroke) {
        if self.held.len() < MAX_HELD {
            self.held.push(keystroke.clone());
        }
    }

    fn end(&mut self) -> Vec<Keystroke> {
        self.until = None;
        std::mem::take(&mut self.held)
    }

    fn is_expired(&self) -> bool {
        self.until.is_some_and(|until| Instant::now() > until)
    }
}

impl FerriteWindow {
    pub(super) fn watch_first_keys(window: &Window, cx: &mut Context<Self>) -> Subscription {
        let this = cx.weak_entity();
        let own = window.window_handle();
        cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle() != own {
                return;
            }
            let held = this
                .update(cx, |view, cx| view.hold_first_key(event, window, cx))
                .unwrap_or(false);
            if held {
                cx.stop_propagation();
            }
        })
    }

    fn hold_first_key(
        &mut self,
        event: &KeystrokeEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.first_keys.until.is_none() {
            return false;
        }
        if self.first_keys.is_expired() {
            self.first_keys.end();
            return false;
        }
        let is_ready = self.first_listing_ready();
        if self.first_keys.held.is_empty() && (is_ready || !self.pane_has_focus(window)) {
            if is_ready {
                self.first_keys.end();
            }
            return false;
        }
        self.first_keys.hold(&event.keystroke);
        if is_ready {
            self.replay_first_keys(cx);
        }
        true
    }

    pub(super) fn release_first_keys(&mut self, cx: &mut Context<Self>) {
        if self.first_keys.until.is_none() {
            return;
        }
        if self.first_keys.is_expired() {
            self.first_keys.end();
        } else if self.first_listing_ready() {
            self.replay_first_keys(cx);
        }
    }

    fn replay_first_keys(&mut self, cx: &mut Context<Self>) {
        let keys = self.first_keys.end();
        if keys.is_empty() {
            return;
        }
        let handle = self.first_keys.window;
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| replay(window, keys, cx));
        });
    }

    fn first_listing_ready(&self) -> bool {
        let tab = self.browser.state().active_tab();
        tab.listing().is_some()
            || tab.pending_request().is_none()
            || self.browser.error(tab.id()).is_some()
    }
}

fn replay(window: &mut Window, keys: Vec<Keystroke>, cx: &mut App) {
    window.refresh();
    for key in keys {
        window.dispatch_keystroke(key, cx);
    }
}
