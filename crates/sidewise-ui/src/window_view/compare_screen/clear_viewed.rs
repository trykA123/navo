//! "Clear all viewed marks" (VD2): clears the app's Viewed store off the UI thread, drops the
//! marks shown on this Stream and tells the window, which toasts and clears the other tabs.

use navo_l10n::t;

use crate::prelude::*;
use crate::window_view::{ScreenEvent, ScreenOwner};

/// The toast for the end of a clear, and whether any store was emptied.
fn outcome_of(result: &Result<usize, String>) -> (String, bool) {
    match result {
        Ok(0) => (t!("compare-viewed-none"), true),
        Ok(count) => (t!("compare-viewed-cleared", count = *count), true),
        Err(reason) => (
            t!("compare-viewed-clear-failed", reason = reason.as_str()),
            false,
        ),
    }
}

impl ScreenOwner {
    /// Clears every Viewed mark on a background task; when it ends, drops this Stream's marks
    /// and emits `ViewedCleared`. Does nothing without diff hooks.
    pub(super) fn start_clear_viewed(&mut self, cx: &mut Context<Self>) {
        let Some(hooks) = self.compare.diff.hooks.clone() else {
            return;
        };
        let clearing = cx.background_spawn(async move { hooks.clear_viewed() });
        cx.spawn(async move |this, cx| {
            let result = clearing.await;
            let _ = this.update(cx, |view, cx| view.finish_clear_viewed(&result, cx));
        })
        .detach();
    }

    /// Drops this screen's Viewed marks after a successful clear and reports the outcome.
    fn finish_clear_viewed(&mut self, result: &Result<usize, String>, cx: &mut Context<Self>) {
        let (text, cleared) = outcome_of(result);
        if cleared {
            self.viewed_cleared(cx);
        }
        cx.emit(ScreenEvent::ViewedCleared { text, cleared });
    }

    /// Drops every Viewed mark this screen shows, after the store was cleared from any tab.
    pub fn viewed_cleared(&mut self, cx: &mut Context<Self>) {
        let Some(stream) = self.stream_mut() else {
            return;
        };
        stream.model.clear_viewed();
        stream.sync_rows();
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
