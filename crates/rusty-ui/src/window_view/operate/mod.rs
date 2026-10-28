use std::time::Duration;

use rusty_core::{Location, OpKind, OperationPlan};
use rusty_fs::CopyMode;
use rusty_l10n::{t, tr};

use super::FerriteWindow;
use crate::activity::ActivityId;
use crate::browser::{PendingOp, UndoOffer};
use crate::confirm_dialog::{Choice, ConfirmDialog, ConfirmProps};
use crate::format;
use crate::prelude::*;

pub(super) const JOB_LINGER: Duration = Duration::from_secs(2);

fn items(count: usize) -> String {
    format::count(u64::try_from(count).unwrap_or(u64::MAX), "item", "items")
}

fn target_text(target: Option<&Location>) -> &str {
    target.map_or("", |target| target.as_str())
}

fn oversized_retained_delete(pending: &PendingOp) -> bool {
    pending
        .retain_deletes
        .is_some_and(|policy| rusty_core::retention::delete_exceeds_capacity(&pending.plan, policy))
}

fn question(pending: &PendingOp) -> (String, String) {
    let count = items(pending.sources.len());
    let target = target_text(pending.target.as_ref());
    match pending.plan.kind {
        OpKind::Copy => (format!("Copy {count} to {target}?"), "Copy".to_owned()),
        OpKind::Move => (format!("Move {count} to {target}?"), "Move".to_owned()),
        OpKind::Recycle => (
            format!("Move {count} to the Recycle Bin?"),
            "Recycle".to_owned(),
        ),
        OpKind::Delete if oversized_retained_delete(pending) => {
            (tr("job-delete-too-large-title"), tr("cmd-files-delete"))
        }
        OpKind::Delete if pending.retain_deletes.is_some() => (
            t!("job-delete-retained-title", items = count.as_str()),
            tr("job-delete-retained-label"),
        ),
        OpKind::Delete => (
            format!("Delete {count} permanently?"),
            "Delete permanently".to_owned(),
        ),
        OpKind::Rename => (
            t!("files-rename-confirm", items = count.as_str()),
            "Rename".to_owned(),
        ),
        OpKind::Link => (
            t!(
                "files-link-confirm",
                items = count.as_str(),
                target = target
            ),
            tr("files-link-label"),
        ),
    }
}

fn capacity_warning(pending: &PendingOp) -> String {
    if rusty_core::retention::delete_exceeds_limit(&pending.plan) {
        return t!(
            "job-delete-too-large-help",
            limit = rusty_core::retention::MAX_NODES.to_string()
        );
    }
    let limit = pending
        .retain_deletes
        .map_or(0, |policy| policy.max_volume_bytes());
    t!("job-delete-over-capacity-help", limit = format::size(limit))
}

pub(super) fn confirm_props(pending: &PendingOp) -> ConfirmProps {
    let (title, label) = question(pending);
    let protected = pending.is_protected();
    let props = ConfirmProps {
        title: title.into(),
        lines: pending
            .sources
            .iter()
            .map(|source| source.as_str().to_owned().into())
            .collect(),
        more: 0,
        summary: match pending.retain_deletes {
            _ if oversized_retained_delete(pending) => capacity_warning(pending),
            Some(policy) if pending.plan.kind == OpKind::Delete => format!(
                "{} · {}",
                summary(&pending.plan, protected),
                t!("job-delete-retained-help", days = policy.days())
            ),
            _ => summary(&pending.plan, protected),
        }
        .into(),
        confirm_label: label.into(),
        danger: pending.plan.kind == OpKind::Delete || protected,
        typed: protected.then(|| "DELETE".into()),
        mode: None,
        choice: pending.structure.as_ref().map(|structure| Choice {
            label: tr("files-keep-structure").into(),
            on: structure.keep,
        }),
    };
    match &pending.undo {
        Some(offer) => super::undo::ask_props(props, offer),
        None => props,
    }
}

pub(super) fn mode_label(kind: OpKind, mode: CopyMode) -> Option<SharedString> {
    matches!(kind, OpKind::Copy | OpKind::Move).then(|| mode.label().into())
}

fn summary(plan: &OperationPlan, protected: bool) -> String {
    let totals = &plan.totals;
    let mut parts = Vec::new();
    if protected {
        parts.push("This is a protected location".to_owned());
    }
    parts.push(format!(
        "{}, {}",
        format::count(totals.files, "file", "files"),
        format::count(totals.folders, "folder", "folders")
    ));
    parts.push(format::size(totals.bytes));
    if !plan.conflicts.is_empty() {
        let count = items(plan.conflicts.len());
        parts.push(format!("{count} already there will be skipped"));
    }
    parts.join(" · ")
}

impl FerriteWindow {
    pub(super) fn dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pending = self.browser.pending()?;
        let mut props = confirm_props(pending);
        props.mode = mode_label(pending.plan.kind, self.browser.copy_mode());
        let (confirm, cancel, toggle) = (cx.weak_entity(), cx.weak_entity(), cx.weak_entity());
        let dialog = ConfirmDialog::new(("confirm", pending.id), props)
            .on_confirm(move |window, cx| {
                let _ = confirm.update(cx, |view, cx| view.close_dialog(true, window, cx));
            })
            .on_cancel(move |window, cx| {
                let _ = cancel.update(cx, |view, cx| view.close_dialog(false, window, cx));
            })
            .on_toggle(move |_, cx| {
                let _ = toggle.update(cx, |view, cx| {
                    view.browser.toggle_structure();
                    cx.notify();
                });
            });
        Some(
            div()
                .id("scrim")
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

    fn close_dialog(&mut self, confirmed: bool, window: &mut Window, cx: &mut Context<Self>) {
        if confirmed && self.browser.pending().is_some_and(PendingOp::is_planning) {
            return;
        }
        if confirmed {
            let undone = self.browser.pending().and_then(|op| op.undo.as_ref());
            let undone = undone.map(UndoOffer::items);
            self.browser.confirm();
            if let Some(items) = undone {
                self.job_ui.activity.retire_batch(items);
            }
            self.refresh_activity(cx);
        } else {
            self.browser.dismiss();
        }
        self.focus_core_pane(window, cx);
        cx.notify();
    }

    pub(super) fn linger_ended(&mut self, cx: &mut Context<Self>) {
        let ended = self.browser.take_ended();
        if ended.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(JOB_LINGER).await;
            let _ = this.update(cx, |view, cx| view.forget_jobs(&ended, cx));
        })
        .detach();
    }

    fn forget_jobs(&mut self, ids: &[u64], cx: &mut Context<Self>) {
        if self.browser.forget_jobs(ids) {
            self.jobs_changed();
            self.refresh_activity(cx);
        }
    }

    pub(super) fn dismiss_job(&mut self, id: u64, cx: &mut Context<Self>) {
        self.job_ui.activity.dismiss(ActivityId::Job(id));
        if self.browser.dismiss_job(id) {
            self.jobs_changed();
        }
        self.refresh_activity(cx);
    }

    pub(super) fn pause_job(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.browser.pause_job(id) {
            self.refresh_activity(cx);
        }
    }

    pub(super) fn resume_job(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.browser.resume_job(id) {
            self.refresh_activity(cx);
        }
    }

    pub(super) fn cancel_job(&mut self, id: u64, cx: &mut Context<Self>) {
        self.browser.cancel_job(id);
        self.refresh_activity(cx);
    }
}

#[cfg(test)]
mod tests;
