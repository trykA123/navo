use gpui_kit::Subscription;

use super::NavoWindow;
use crate::prelude::*;

impl NavoWindow {
    pub(super) fn watches(window: &mut Window, cx: &mut Context<Self>) -> [Subscription; 7] {
        [
            cx.observe_in(&cx.entity(), window, |view, entity, window, cx| {
                view.close_moved_path_edit(cx);
                view.follow_cursor(entity, window, cx);
                view.queue_cached_refresh(window, cx);
            }),
            cx.observe_window_activation(window, Self::activation_changed),
            cx.observe_window_appearance(window, Self::appearance_changed),
            cx.observe_window_bounds(window, Self::viewport_changed),
            cx.on_focus_lost(window, Self::popup_focus_lost),
            Self::watch_dialog_input(window, cx),
            Self::watch_first_keys(window, cx),
        ]
    }

    /// Polls the browser, jobs and preview after a background wake-up; redraws when one changed.
    pub(super) fn wake(&mut self, cx: &mut Context<Self>) {
        let polled = self.browser.poll() | self.show_compare_notice();
        self.note_seen(cx);
        self.clear_finished_cuts();
        self.sync_git();
        self.jobs_woke(cx);
        if polled {
            self.drag_list_changed(cx);
        }
        if self.poll_preview() | self.host_poll(cx) || polled {
            cx.notify();
        }
        self.linger_ended(cx);
        self.release_first_keys(cx);
    }

    fn appearance_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_theme(window, cx);
        cx.notify();
    }

    fn activation_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.hint_buttons_free();
        self.track_modifiers(window.modifiers(), window, cx);
        if window.is_window_active() {
            self.recheck_path_focus();
            self.restart_index_activity(cx);
            self.apply_theme(window, cx);
            self.browser.revalidate_unwatched();
            self.refresh_drives(cx);
            if let Some(git) = self.git_badges.hooks() {
                git.focus_regained();
            }
            cx.notify();
        }
    }
}
