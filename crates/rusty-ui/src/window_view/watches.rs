use gpui_kit::Subscription;

use super::FerriteWindow;
use crate::prelude::*;

impl FerriteWindow {
    pub(super) fn watches(window: &mut Window, cx: &mut Context<Self>) -> [Subscription; 6] {
        [
            cx.observe_in(&cx.entity(), window, |view, entity, window, cx| {
                view.follow_cursor(entity, window, cx);
                view.queue_cached_refresh(window, cx);
            }),
            cx.observe_window_activation(window, Self::activation_changed),
            cx.observe_window_appearance(window, Self::appearance_changed),
            cx.observe_window_bounds(window, Self::viewport_changed),
            cx.on_focus_lost(window, Self::popup_focus_lost),
            Self::watch_dialog_input(window, cx),
        ]
    }

    fn appearance_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_theme(window, cx);
        cx.notify();
    }

    fn activation_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.track_modifiers(window.modifiers(), window, cx);
        if window.is_window_active() {
            self.restart_index_activity(cx);
            self.apply_theme(window, cx);
            self.browser.revalidate_unwatched();
            self.refresh_drives(cx);
            cx.notify();
        }
    }
}
