use std::time::Duration;

use futures::FutureExt as _;
use futures::future::Shared;
use gpui_kit::{Subscription, Task};

use super::{BackgroundSaveSession, NavoWindow};
use crate::prelude::*;

const JOB_STOP_WAIT: Duration = Duration::from_secs(10);
const SAVE_AFTER: Duration = Duration::from_secs(2);

pub(super) struct Autosave {
    pub(super) save: Option<BackgroundSaveSession>,
    wait: Option<Task<()>>,
    in_flight: Option<Shared<Task<()>>>,
    _changes: Subscription,
}

impl Autosave {
    pub(super) fn observe(cx: &mut Context<NavoWindow>) -> Self {
        Self {
            save: None,
            wait: None,
            in_flight: None,
            _changes: cx.observe_self(|view, cx| view.watch_session(cx)),
        }
    }
}

impl NavoWindow {
    fn watch_session(&mut self, cx: &mut Context<Self>) {
        if self.autosave.wait.is_some() || !self.browser.take_save_request() {
            return;
        }
        self.autosave.wait = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_AFTER).await;
            let Ok(task) = this.update(cx, Self::begin_session_save) else {
                return;
            };
            if let Some(task) = task {
                task.await;
            }
            let _ = this.update(cx, |view, cx| {
                view.autosave.in_flight = None;
                view.autosave.wait = None;
                view.watch_session(cx);
            });
        }));
    }

    fn begin_session_save(&mut self, cx: &mut Context<Self>) -> Option<Shared<Task<()>>> {
        self.browser.take_save_request();
        let Some(save) = self.autosave.save.clone() else {
            self.save();
            return None;
        };
        let state = self.browser.state().clone();
        let task = cx
            .background_executor()
            .spawn(async move { save(&state) })
            .shared();
        self.autosave.in_flight = Some(task.clone());
        Some(task)
    }

    pub(super) fn save(&self) {
        (self.on_save)(self.browser.state());
    }

    pub(super) fn close(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        if self.warn_unsaved(cx) {
            return false;
        }
        self.save_recording_on_close(cx);
        let _ = self.browser.shutdown(JOB_STOP_WAIT);
        self.save_now(window, cx);
        true
    }

    fn save_now(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.remember_placement(window, cx);
        self.autosave.wait = None;
        self.browser.take_save_request();
        if let Some(task) = self.autosave.in_flight.take() {
            cx.foreground_executor().block_on(task);
        }
        self.save();
    }

    pub(super) fn close_or_hide(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        if self.settings.updates.restart == Restart::Applying {
            return false;
        }
        let has_tray = self
            .resident
            .as_ref()
            .is_some_and(|resident| resident.has_tray());
        if !self.values.keeps_in_tray() || self.window_shown.is_none() || !has_tray {
            return self.close(window, cx);
        }
        self.save_now(window, cx);
        !self.set_window_shown(window, false) && self.close(window, cx)
    }

    pub(super) fn close_saves_and_quits(window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            let closing = this
                .update(cx, |view, cx| view.close_or_hide(window, cx))
                .unwrap_or(true);
            if closing {
                cx.quit();
            }
            closing
        });
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Restart {
    #[default]
    Idle,
    Asking,
    Waiting,
    Saving,
    Applying,
}

impl NavoWindow {
    pub(super) fn restart_available(&self) -> bool {
        self.identity.install == crate::app_menu::Install::Installed
            && matches!(self.settings.updates.view, super::UpdateView::Ready { .. })
    }

    /// The Restart offers in Help and Ctrl+K: available and not already under way.
    pub(super) fn restart_offered(&self) -> bool {
        self.restart_available() && self.settings.updates.restart == Restart::Idle
    }

    pub(super) fn restart_applying(&self) -> bool {
        self.settings.updates.restart == Restart::Applying
    }

    /// Jobs, an undo restore, Recently deleted work and a running Git action all hold a restart.
    pub(super) fn restart_busy(&self) -> bool {
        self.browser.running()
            || self.job_ui.is_restoring
            || self.recent.working_count() > 0
            || self.git_menu.running > 0
    }

    pub(super) fn restart_entry(&self) -> Option<crate::commands::CommandEntry> {
        self.restart_offered()
            .then_some(crate::commands::CommandEntry {
                spec: crate::commands::CommandSpec {
                    name: "app.restart_update",
                    label: "cmd-app-restart-update",
                    context: crate::commands::GLOBAL,
                    action: Some("updates::Restart"),
                },
                enabled: true,
                in_palette: true,
                arg: None,
            })
    }

    pub(super) fn restart_job_count(&self) -> usize {
        let jobs = self
            .browser
            .jobs()
            .iter()
            .filter(|job| job.status.is_active())
            .count();
        let work = jobs
            + usize::from(self.job_ui.is_restoring)
            + self.recent.working_count()
            + self.git_menu.running;
        work.max(usize::from(self.restart_busy()))
    }

    pub(super) fn request_restart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.restart_available() || self.settings.updates.restart != Restart::Idle {
            return;
        }
        let entity = cx.entity();
        self.settings.updates.restart_watch =
            Some(cx.observe_in(&entity, window, |view, _, window, cx| {
                view.continue_restart(window, cx);
            }));
        if self.restart_busy() {
            self.browser.dialog_opened();
            self.settings.updates.restart_dialogs += 1;
            self.settings.updates.restart = Restart::Asking;
        } else {
            self.settings.updates.restart = Restart::Waiting;
            self.continue_restart(window, cx);
        }
        cx.notify();
    }

    pub(super) fn cancel_restart(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.settings.updates.restart,
            Restart::Asking | Restart::Waiting
        ) {
            self.settings.updates.restart = Restart::Idle;
            self.settings.updates.restart_watch = None;
            cx.notify();
        }
    }

    pub(super) fn continue_restart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.updates.restart == Restart::Asking && !self.restart_busy() {
            self.settings.updates.restart = Restart::Waiting;
            self.focus_core_pane(window, cx);
        }
        if self.settings.updates.restart == Restart::Waiting && !self.restart_busy() {
            if !self.restart_available() {
                self.drop_restart(None);
                return;
            }
            if self.warn_unsaved(cx) {
                self.drop_restart(Some("update-restart-stopped-unsaved"));
                return;
            }
            self.save_recording_on_close(cx);
            self.save_now(window, cx);
            self.settings.updates.restart = Restart::Saving;
        }
        if self.settings.updates.restart != Restart::Saving || self.saver.is_busy() {
            return;
        }
        if self.restart_busy() {
            self.settings.updates.restart = Restart::Waiting;
            return;
        }
        if self.saver.error.is_some() {
            self.drop_restart(Some("update-restart-stopped-save"));
            return;
        }
        if !self.restart_available() {
            self.drop_restart(None);
            return;
        }
        self.apply_restart(window, cx);
    }

    /// Ends a restart that cannot go on, saying why when the user would not otherwise see it.
    fn drop_restart(&mut self, why: Option<&'static str>) {
        self.settings.updates.restart = Restart::Idle;
        self.settings.updates.restart_watch = None;
        if let Some(key) = why {
            self.show_notice(crate::status_bar::StatusNotice {
                text: navo_l10n::tr(key).into(),
                is_error: true,
            });
        }
    }

    fn apply_restart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.updates.restart = Restart::Applying;
        self.settings.updates.restart_watch = None;
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.settings.updates.restart_focus = Some(focus);
        let restart = self.settings.updates.hooks.restart.clone();
        let task = cx.background_executor().spawn(async move { restart() });
        self.settings.updates.restart_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.finish_restart(result, window, cx)
            });
        }));
    }

    fn finish_restart(
        &mut self,
        result: Result<(), super::UpdateFailure>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings.updates.restart = Restart::Idle;
        self.settings.updates.restart_task = None;
        self.settings.updates.restart_focus = None;
        let Err(failure) = result else {
            window.remove_window();
            cx.quit();
            return;
        };
        self.settings.updates.view = super::UpdateView::Failed(failure);
        self.forget_update_notice();
        if self.settings.is_open() {
            self.focus_settings(window, cx);
        } else {
            self.focus_core_pane(window, cx);
        }
        cx.notify();
    }

    fn notice_restart(&mut self, waiting: bool, window: &mut Window, cx: &mut Context<Self>) {
        if waiting {
            self.cancel_restart(cx);
        } else {
            self.request_restart(window, cx);
        }
    }

    pub(super) fn restart_notice_action(
        &self,
        shown: Option<&crate::status_bar::StatusNotice>,
        cx: &mut Context<Self>,
    ) -> Option<crate::primitives::Button> {
        let waiting = self.settings.updates.restart == Restart::Waiting;
        let offers = self.restart_available() && shown == self.update_notice().as_ref();
        if shown.is_none() || !(waiting || offers) {
            return None;
        }
        let this = cx.weak_entity();
        let (id, key) = if waiting {
            ("restart-cancel", "update-restart-cancel")
        } else {
            ("restart-update", "update-restart")
        };
        Some(
            crate::primitives::Button::new(id, navo_l10n::tr(key)).on_click(
                move |_, window, cx| {
                    let _ = this.update(cx, |view, cx| view.notice_restart(waiting, window, cx));
                },
            ),
        )
    }

    pub(super) fn restart_dialog_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if let Some(focus) = &self.settings.updates.restart_focus {
            return Some(
                div()
                    .id("restart-applying")
                    .absolute()
                    .inset_0()
                    .occlude()
                    .track_focus(focus)
                    .key_context("Dialog")
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(crate::tokens::current(cx).surfaces.scrim)
                    .capture_key_down(|_, _, cx| cx.stop_propagation())
                    .child(navo_l10n::tr("update-restarting"))
                    .into_any_element(),
            );
        }
        if self.settings.updates.restart != Restart::Asking {
            return None;
        }
        Some(self.restart_jobs_dialog(cx))
    }

    fn restart_jobs_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        use crate::confirm_dialog::{ConfirmDialog, ConfirmProps};
        let props = ConfirmProps {
            title: navo_l10n::t!("update-restart-jobs", count = self.restart_job_count()).into(),
            lines: Vec::new(),
            more: 0,
            summary: navo_l10n::tr("update-restart-summary").into(),
            confirm_label: navo_l10n::tr("update-restart-when-idle").into(),
            danger: false,
            typed: None,
            mode: None,
            choice: None,
        };
        let (arm, cancel) = (cx.weak_entity(), cx.weak_entity());
        let dialog = ConfirmDialog::new(
            (
                "restart-dialog-state",
                self.settings.updates.restart_dialogs,
            ),
            props,
        )
        .on_confirm(move |window, cx| {
            let _ = arm.update(cx, |view, cx| {
                view.settings.updates.restart = Restart::Waiting;
                view.focus_core_pane(window, cx);
                view.continue_restart(window, cx);
                cx.notify();
            });
        })
        .on_cancel(move |window, cx| {
            let _ = cancel.update(cx, |view, cx| {
                view.cancel_restart(cx);
                view.focus_core_pane(window, cx);
            });
        });
        div()
            .id("restart-dialog")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(crate::tokens::current(cx).surfaces.scrim)
            .occlude()
            .child(dialog)
            .into_any_element()
    }
}

#[cfg(test)]
#[path = "closing_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "restart_tests.rs"]
mod restart_tests;
