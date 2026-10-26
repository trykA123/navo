use std::time::Duration;

use super::FerriteWindow;
use crate::prelude::*;

const JOB_STOP_WAIT: Duration = Duration::from_secs(10);

impl FerriteWindow {
    pub(super) fn save(&self) {
        (self.on_save)(self.browser.state());
    }

    pub(super) fn close(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        if self.warn_unsaved(cx) {
            return false;
        }
        self.save_recording_on_close(cx);
        let _ = self.browser.shutdown(JOB_STOP_WAIT);
        self.remember_placement(window, cx);
        self.save();
        true
    }

    pub(super) fn close_saves_and_quits(window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            let closing = this
                .update(cx, |view, cx| view.close(window, cx))
                .unwrap_or(true);
            if closing {
                cx.quit();
            }
            closing
        });
    }
}
