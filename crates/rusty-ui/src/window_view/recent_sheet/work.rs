use std::rc::Rc;

use rusty_core::Location;
use rusty_l10n::{t, tr};

use super::super::FerriteWindow;
use crate::browser::{RetainedRow, RetainedState};
use crate::confirm_dialog::{ConfirmDialog, ConfirmProps};
use crate::format;
use crate::prelude::*;
use crate::recently_deleted::{folder_of, name_of};
use crate::status_bar::StatusNotice;

fn refusal(row: &RetainedRow) -> Option<String> {
    match row.state {
        RetainedState::Ready => None,
        RetainedState::Busy => Some(tr("retained-busy")),
        RetainedState::NeedsAttention => Some(tr("retained-not-ready")),
    }
}

impl FerriteWindow {
    fn actionable(&mut self) -> Option<RetainedRow> {
        let row = self.recent.current()?.clone();
        if self.recent.working.contains(&row.id) {
            return None;
        }
        if let Some(reason) = refusal(&row) {
            self.recent.status = Some(reason.into());
            return None;
        }
        self.recent.status = None;
        Some(row)
    }

    pub(super) fn restore_current(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.actionable() else {
            return;
        };
        Rc::make_mut(&mut self.recent.working).push(row.id);
        let hooks = self.browser.retained_hooks();
        let id = row.id;
        let restoring = cx.background_spawn(async move { hooks.restore(id) });
        cx.spawn(async move |this, cx| {
            let outcome = restoring.await;
            let _ = this.update(cx, |view, cx| view.sheet_restored((&row, outcome), cx));
        })
        .detach();
    }

    fn sheet_restored(
        &mut self,
        (row, outcome): (&RetainedRow, Result<Location, String>),
        cx: &mut Context<Self>,
    ) {
        self.recent.end_work(row.id);
        let notice = match outcome {
            Ok(place) => {
                self.recent.drop_row(row.id);
                if let Some(folder) = place.parent() {
                    self.browser.refresh_unwatched(&[folder]);
                }
                StatusNotice {
                    text: t!(
                        "recently-deleted-restored",
                        name = name_of(&place),
                        folder = folder_of(&place)
                    )
                    .into(),
                    is_error: false,
                }
            }
            Err(error) => self.failed_work(error, cx),
        };
        self.work_ended(notice, cx);
    }

    fn failed_work(&mut self, error: String, cx: &mut Context<Self>) -> StatusNotice {
        if self.recent.is_open {
            self.load_retained(cx);
        }
        StatusNotice {
            text: error.into(),
            is_error: true,
        }
    }

    fn work_ended(&mut self, notice: StatusNotice, cx: &mut Context<Self>) {
        if self.recent.is_open {
            self.recent.status = Some(notice.text);
        } else {
            self.show_notice(notice);
        }
        cx.notify();
    }

    pub(super) fn ask_purge_current(&mut self) {
        if let Some(row) = self.actionable() {
            self.recent.asks += 1;
            self.recent.ask = Some(row);
        }
    }

    fn answer_purge(&mut self, is_confirmed: bool, window: &mut Window, cx: &mut Context<Self>) {
        let asked = self.recent.ask.take();
        self.focus_recent(window, cx);
        if let (true, Some(row)) = (is_confirmed, asked) {
            self.purge_row(row, cx);
        }
        cx.notify();
    }

    fn purge_row(&mut self, row: RetainedRow, cx: &mut Context<Self>) {
        Rc::make_mut(&mut self.recent.working).push(row.id);
        let hooks = self.browser.retained_hooks();
        let id = row.id;
        let purging = cx.background_spawn(async move { hooks.purge(id) });
        cx.spawn(async move |this, cx| {
            let outcome = purging.await;
            let _ = this.update(cx, |view, cx| view.sheet_purged((&row, outcome), cx));
        })
        .detach();
    }

    fn sheet_purged(
        &mut self,
        (row, outcome): (&RetainedRow, Result<(), String>),
        cx: &mut Context<Self>,
    ) {
        self.recent.end_work(row.id);
        let notice = match outcome {
            Ok(()) => {
                self.recent.drop_row(row.id);
                StatusNotice {
                    text: t!("recently-deleted-purged", name = name_of(&row.original)).into(),
                    is_error: false,
                }
            }
            Err(error) => self.failed_work(error, cx),
        };
        self.work_ended(notice, cx);
    }

    pub(in crate::window_view) fn recent_ask_view(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let row = self.recent.ask.as_ref()?;
        let props = ConfirmProps {
            title: t!(
                "recently-deleted-delete-title",
                name = name_of(&row.original)
            )
            .into(),
            lines: vec![row.original.as_str().to_owned().into()],
            more: 0,
            summary: t!(
                "recently-deleted-delete-summary",
                size = format::size(row.bytes)
            )
            .into(),
            confirm_label: tr("recently-deleted-delete-confirm").into(),
            danger: true,
            typed: None,
            mode: None,
            choice: None,
        };
        let (confirm, cancel) = (cx.weak_entity(), cx.weak_entity());
        let dialog = ConfirmDialog::new(("recent-ask", self.recent.asks), props)
            .on_confirm(move |window, cx| {
                let _ = confirm.update(cx, |view, cx| view.answer_purge(true, window, cx));
            })
            .on_cancel(move |window, cx| {
                let _ = cancel.update(cx, |view, cx| view.answer_purge(false, window, cx));
            });
        Some(
            div()
                .id("recent-ask-scrim")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(gpui_kit::black().opacity(0.4))
                .occlude()
                .child(dialog)
                .into_any_element(),
        )
    }
}
