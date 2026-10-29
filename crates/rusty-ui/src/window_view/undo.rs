use rusty_core::Step;
use rusty_l10n::{t, tr};

use super::FerriteWindow;
use super::strip::undo_key;
use super::undo_retained::RetainedUndo;
use crate::browser::{RenameHooks, RetireUndo, UndoKind, UndoOffer};
use crate::confirm_dialog::ConfirmProps;
use crate::file_list::UndoLast;
use crate::format;
use crate::prelude::*;
use crate::status_bar::StatusNotice;

enum UndoQuery {
    Workspace,
    Files {
        offer: Result<Option<UndoOffer>, String>,
        retire: RetireUndo,
    },
    Nothing,
}

fn query(hooks: Option<RenameHooks>, after: Option<Option<u64>>) -> UndoQuery {
    let offer = hooks.as_ref().map_or(Ok(None), |hooks| (hooks.last)());
    let job = offer
        .as_ref()
        .ok()
        .and_then(|offer| offer.as_ref())
        .map(|offer| offer.job);
    if after.is_some_and(|after| after == job) {
        return UndoQuery::Workspace;
    }
    match hooks {
        Some(hooks) => UndoQuery::Files {
            offer,
            retire: std::sync::Arc::new(move |offer| {
                if !(hooks.last)()?.is_some_and(|latest| latest.job == offer.job) {
                    return Err(tr("undo-nothing"));
                }
                (hooks.retire)(offer)
            }),
        },
        None => UndoQuery::Nothing,
    }
}

fn folder_of(offer: &UndoOffer) -> String {
    offer
        .plan
        .steps
        .iter()
        .find_map(|step| match step {
            Step::Rename { target, .. } => target.parent(),
            _ => None,
        })
        .and_then(|folder| folder.name().map(|name| name.to_text()))
        .unwrap_or_default()
}

pub(super) fn ask_props(props: ConfirmProps, offer: &UndoOffer) -> ConfirmProps {
    if offer.restore_pending {
        return ConfirmProps {
            title: tr("undo-finish-ask").into(),
            confirm_label: tr("undo-finish-label").into(),
            summary: tr("undo-finish-summary").into(),
            ..props
        };
    }
    let count = u64::try_from(offer.items()).unwrap_or(u64::MAX);
    ConfirmProps {
        title: t!(
            "undo-ask",
            count = count,
            items = format::bytes(count),
            folder = folder_of(offer)
        )
        .into(),
        confirm_label: tr("undo-ask-label").into(),
        summary: tr("undo-ask-summary").into(),
        ..props
    }
}

impl FerriteWindow {
    pub(super) fn undo_on(root: Div, cx: &mut Context<Self>) -> Div {
        root.on_action(cx.listener(|view, _: &UndoLast, window, cx| {
            view.undo_last(window, cx);
        }))
    }

    fn tell_undo(&mut self, text: String, cx: &mut Context<Self>) {
        self.show_notice(StatusNotice {
            text: text.into(),
            is_error: true,
        });
        cx.notify();
    }

    fn undo_last(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let generation = self.browser.invalidate_undo();
        let is_busy = self.prompt.is_some()
            || self.browser.pending().is_some()
            || self.is_multi_rename_open();
        if is_busy {
            return;
        }
        let hooks = self.browser.rename_hooks().cloned();
        let after = self.workspace_undo_after();
        self.run_off_thread(
            Box::new(move || {
                let result = query(hooks, after);
                Box::new(move |view, window, cx| {
                    view.undo_queried((generation, result), window, cx);
                })
            }),
            window,
            cx,
        );
    }

    fn undo_queried(
        &mut self,
        (generation, result): (u64, UndoQuery),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if generation != self.browser.undo_generation()
            || self.prompt.is_some()
            || self.browser.pending().is_some()
            || self.is_multi_rename_open()
        {
            return;
        }
        if matches!(result, UndoQuery::Workspace) {
            return self.restore_workspace(window, cx);
        }
        if self.job_ui.is_restoring {
            return self.tell_undo(tr("undo-retained-busy"), cx);
        }
        let UndoQuery::Files { offer, retire } = result else {
            return self.tell_undo(tr("undo-nothing"), cx);
        };
        let offer = match offer {
            Ok(Some(offer)) => offer,
            Ok(None) => return self.tell_undo(tr("undo-nothing"), cx),
            Err(reason) => return self.tell_undo(t!("undo-read-failed", reason = reason), cx),
        };
        self.apply_undo_offer(offer, retire, cx);
    }

    fn apply_undo_offer(&mut self, offer: UndoOffer, retire: RetireUndo, cx: &mut Context<Self>) {
        if let UndoKind::Retained(ids) = offer.kind.clone() {
            return self.undo_retained(RetainedUndo { offer, ids, retire }, cx);
        }
        if self.browser.renaming() {
            return self.tell_undo(tr("rename-busy"), cx);
        }
        let items = offer.items();
        let is_at_once = !offer.restore_pending && self.job_ui.activity.shows_batch(items);
        let is_started = if is_at_once {
            self.browser.undo_rename(offer)
        } else {
            self.browser.ask_undo(offer)
        };
        if !is_started {
            return self.tell_undo(tr("undo-nothing"), cx);
        }
        if is_at_once {
            self.job_ui.activity.retire_batch(items);
        }
        self.refresh_activity(cx);
    }

    pub(super) fn tell_pending_undo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(last) = self.browser.rename_hooks().map(|hooks| hooks.last.clone()) else {
            return;
        };
        self.run_off_thread(
            Box::new(move || {
                let pending = matches!(last(), Ok(Some(offer)) if offer.restore_pending);
                Box::new(move |view, _, cx| view.show_pending_undo(pending, cx))
            }),
            window,
            cx,
        );
    }

    fn show_pending_undo(&mut self, pending: bool, cx: &mut Context<Self>) {
        if !pending {
            return;
        }
        let text = match undo_key(cx) {
            Some(key) => t!("undo-pending-notice-key", key = key.to_string()),
            None => tr("undo-pending-notice"),
        };
        self.tell_undo(text, cx);
    }
}
