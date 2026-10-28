use rusty_core::Step;
use rusty_l10n::{t, tr};

use super::FerriteWindow;
use super::strip::undo_key;
use crate::browser::UndoOffer;
use crate::confirm_dialog::ConfirmProps;
use crate::file_list::UndoLast;
use crate::format;
use crate::prelude::*;
use crate::status_bar::StatusNotice;

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
        let is_busy = self.prompt.is_some()
            || self.browser.pending().is_some()
            || self.is_multi_rename_open();
        if is_busy {
            return;
        }
        if self.restores_workspace() {
            return self.restore_workspace(window, cx);
        }
        let Some(last) = self.browser.rename_hooks().map(|hooks| hooks.last.clone()) else {
            return self.tell_undo(tr("undo-nothing"), cx);
        };
        let offer = match last() {
            Ok(Some(offer)) => offer,
            Ok(None) => return self.tell_undo(tr("undo-nothing"), cx),
            Err(reason) => return self.tell_undo(t!("undo-read-failed", reason = reason), cx),
        };
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

    pub(super) fn tell_pending_undo(&mut self, cx: &mut Context<Self>) {
        let Some(last) = self.browser.rename_hooks().map(|hooks| hooks.last.clone()) else {
            return;
        };
        if !matches!(last(), Ok(Some(offer)) if offer.restore_pending) {
            return;
        }
        let text = match undo_key(cx) {
            Some(key) => t!("undo-pending-notice-key", key = key.to_string()),
            None => tr("undo-pending-notice"),
        };
        self.tell_undo(text, cx);
    }
}
