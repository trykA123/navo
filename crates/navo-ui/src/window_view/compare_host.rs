//! The window side of the compare screen: every call between `NavoWindow` and the screen goes
//! through here (NC54, NC55).

use std::ops::ControlFlow;
use std::time::Duration;

use super::NavoWindow;
use super::compare_screen::{CompareScreen, ScreenEvent, items_of, neighbour};
use crate::browser::{Browser, FileDiffRequest};
use crate::compare_diff::PairRequest;
use crate::context_menu::ContextMenu;
use crate::prelude::*;

/// The entity that owns the compare screen's state and rendering.
pub(in crate::window_view) type ScreenOwner = CompareScreen;

impl NavoWindow {
    /// Opens the file diff Enter asked for; false when none was asked or the app has no hooks.
    pub(in crate::window_view) fn open_file_diff(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(asked) = self.browser.take_file_diff() else {
            return false;
        };
        let FileDiffRequest {
            id,
            path,
            left,
            right,
        } = asked;
        let request = PairRequest {
            id,
            path,
            left,
            right,
        };
        self.compare.screen.update(cx, |screen, cx| {
            screen.set_inputs(
                screen_sides_of(&self.browser),
                screen_stamp_of(&self.browser),
            );
            screen.show_pair((request, None), window, cx)
        })
    }

    /// Alt+F or "Stream all differences": opens the Stream over every difference under the
    /// compare folder. False outside a compare or without diff hooks.
    pub(in crate::window_view) fn open_compare_stream(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(items) = self.browser.compare_mode().map(items_of) else {
            return false;
        };
        self.compare.screen.update(cx, |screen, cx| {
            screen.set_inputs(
                screen_sides_of(&self.browser),
                screen_stamp_of(&self.browser),
            );
            screen.open_stream(items, window, cx)
        })
    }

    /// Returns the compare entity in place of the panes, or none while its screen is closed.
    pub(in crate::window_view) fn host_screen_view(
        &mut self,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.compare.screen.read(cx).compare.screen.as_ref()?;
        Some(self.compare.screen.clone().into_any_element())
    }

    /// The open View menu of the file diff or the Stream for the window to draw; `None` while
    /// neither shows one.
    pub(in crate::window_view) fn host_menu_view(&self, cx: &App) -> Option<Entity<ContextMenu>> {
        self.compare.screen.read(cx).compare_menu_view()
    }

    /// Whether the compare screen is open in place of the panes.
    pub(in crate::window_view) fn host_screen_open(&self, cx: &App) -> bool {
        self.compare.screen.read(cx).compare.screen.is_some()
    }

    /// Opens the compare screen's View menu under the window header.
    pub(in crate::window_view) fn host_open_look_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.compare
            .screen
            .update(cx, |screen, cx| screen.open_look_menu(window, cx));
    }

    /// Re-lays the large diff body after a window resize; does nothing with no large body open.
    pub(in crate::window_view) fn host_follow_window(&mut self, window: &Window, cx: &mut App) {
        self.compare
            .screen
            .update(cx, |screen, _| screen.large_follow_window(window));
    }

    /// Resyncs the Stream's items from the folder compare, then drains the Stream's and the
    /// file diff's inboxes; true when the screen changed. Updates of a closed or replaced file
    /// screen are dropped.
    pub(in crate::window_view) fn host_poll(&mut self, cx: &mut Context<Self>) -> bool {
        self.compare.screen.update(cx, |screen, _| {
            screen.set_inputs(
                screen_sides_of(&self.browser),
                screen_stamp_of(&self.browser),
            );
        });
        let resynced = self.host_resync(cx);
        self.compare
            .screen
            .update(cx, |screen, cx| screen.poll_compare_screen(resynced, cx))
    }

    /// Gives focus back to the open file diff; false when none is open.
    pub(in crate::window_view) fn host_focus_file(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        self.compare
            .screen
            .update(cx, |screen, cx| screen.focus_file_screen(window, cx))
    }

    /// Handles one screen event: `Closed` reveals its path and gives focus back to the compare
    /// pane, `OpenSettings` opens Settings at its field, `WalkFile` opens the folder compare's
    /// next or previous file, `Resync` rebuilds the Stream's items. `window` is `None` only for
    /// `Resync` (N4f2).
    pub(in crate::window_view) fn on_screen_event(
        &mut self,
        event: &ScreenEvent,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        match event {
            ScreenEvent::Closed { reveal } => {
                if let Some(path) = reveal {
                    self.browser.compare_reveal(path);
                }
                if let Some(window) = window {
                    self.focus_core_pane(window, cx);
                }
                cx.notify();
            }
            ScreenEvent::OpenSettings(field) => {
                if let Some(window) = window {
                    self.open_settings_at(*field, window, cx);
                }
            }
            ScreenEvent::WalkFile { from, step } => {
                let Some(window) = window else {
                    return;
                };
                self.host_walk_file((*from, *step), window, cx);
            }
            ScreenEvent::Resync => {
                if self.host_resync(cx) {
                    cx.notify();
                }
            }
        }
    }

    /// Opens the folder compare's neighbour of row `from` in direction `step`, when present.
    fn host_walk_file(
        &mut self,
        (from, step): (u64, i32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let found = self
            .browser
            .compare_mode()
            .and_then(|mode| neighbour(mode, from, step));
        let Some(request) = found else {
            return;
        };
        self.compare.screen.update(cx, |screen, cx| {
            screen.set_inputs(
                screen_sides_of(&self.browser),
                screen_stamp_of(&self.browser),
            );
            screen.show_pair((request, Some(0)), window, cx);
        });
    }

    /// Rebuilds the Stream after a changed rows stamp, throttling batches while checking continues.
    pub(in crate::window_view) fn host_resync(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(stamp) = self.compare.screen.read(cx).screen_stamp() else {
            return false;
        };
        let checking = self
            .browser
            .compare_mode()
            .is_some_and(|mode| mode.checking());
        let now = cx.background_executor().now();
        let Some(step) = self
            .compare
            .screen
            .read(cx)
            .resync_step(stamp, checking, now)
        else {
            return false;
        };
        match step {
            ControlFlow::Break(wait) => {
                self.arm_resync(wait, cx);
                false
            }
            ControlFlow::Continue(fresh) => {
                let Some(items) = self.browser.compare_mode().map(items_of) else {
                    return false;
                };
                self.compare.screen.update(cx, |screen, cx| {
                    screen.apply_resync_items(items, (stamp, now, fresh), cx)
                })
            }
        }
    }

    /// Attaches a host resync timer to the Stream so closing it cancels the wait.
    fn arm_resync(&mut self, wait: Duration, cx: &mut Context<Self>) {
        if !self.compare.screen.read(cx).needs_resync_wait() {
            return;
        }
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, Self::resync_due);
        });
        self.compare
            .screen
            .update(cx, |screen, _| screen.set_resync_wait(task));
    }

    /// Releases the completed wait and resyncs the Stream from the browser's current rows.
    fn resync_due(&mut self, cx: &mut Context<Self>) {
        self.compare
            .screen
            .update(cx, |screen, _| screen.finish_resync_wait());
        self.on_screen_event(&ScreenEvent::Resync, None, cx);
    }
}

/// The compare roots' names as "left ↔ right", or empty outside a compare.
fn screen_sides_of(browser: &Browser) -> SharedString {
    let Some(mode) = browser.compare_mode() else {
        return SharedString::default();
    };
    let [left, right] = mode.roots().clone().map(|root| {
        root.name()
            .map_or_else(|| root.as_str().to_owned(), |name| name.to_text())
    });
    navo_l10n::t!("compare-screen-sides", left = left, right = right).into()
}

/// The compare's generation and received batches, or none outside a compare.
fn screen_stamp_of(browser: &Browser) -> Option<(u64, u64)> {
    browser.compare_mode().map(|mode| mode.rows_stamp())
}
