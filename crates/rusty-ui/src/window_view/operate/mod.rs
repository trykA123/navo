use std::time::Duration;

use rusty_core::{Location, OpKind, OperationPlan, Risk};

use super::FerriteWindow;
use crate::browser::PendingOp;
use crate::confirm_dialog::{ConfirmDialog, ConfirmProps};
use crate::format;
use crate::prelude::*;

pub(super) const JOB_LINGER: Duration = Duration::from_secs(2);

fn items(count: usize) -> String {
    format::count(u64::try_from(count).unwrap_or(u64::MAX), "item", "items")
}

fn is_protected(plan: &OperationPlan, target: Option<&Location>) -> bool {
    plan.risks
        .iter()
        .any(|risk| matches!(risk, Risk::Protected(place) if Some(place) != target))
}

fn target_text(target: Option<&Location>) -> &str {
    target.map_or("", |target| target.as_str())
}

pub(super) fn confirm_props(pending: &PendingOp) -> ConfirmProps {
    let count = items(pending.sources.len());
    let target = target_text(pending.target.as_ref());
    let (title, label) = match pending.plan.kind {
        OpKind::Copy => (format!("Copy {count} to {target}?"), "Copy"),
        OpKind::Move => (format!("Move {count} to {target}?"), "Move"),
        OpKind::Recycle => (format!("Move {count} to the Recycle Bin?"), "Recycle"),
        OpKind::Delete => (format!("Delete {count} permanently?"), "Delete permanently"),
    };
    let protected = is_protected(&pending.plan, pending.target.as_ref());
    ConfirmProps {
        title: title.into(),
        lines: pending
            .sources
            .iter()
            .map(|source| source.as_str().to_owned().into())
            .collect(),
        more: 0,
        summary: summary(&pending.plan, protected).into(),
        confirm_label: label.into(),
        danger: pending.plan.kind == OpKind::Delete || protected,
        typed: protected.then(|| "DELETE".into()),
    }
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
        let props = confirm_props(pending);
        let (confirm, cancel) = (cx.weak_entity(), cx.weak_entity());
        let dialog = ConfirmDialog::new(("confirm", pending.id), props)
            .on_confirm(move |window, cx| {
                let _ = confirm.update(cx, |view, cx| view.close_dialog(true, window, cx));
            })
            .on_cancel(move |window, cx| {
                let _ = cancel.update(cx, |view, cx| view.close_dialog(false, window, cx));
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
        if confirmed {
            self.browser.confirm();
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
            cx.notify();
        }
    }

    pub(super) fn dismiss_job(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.browser.dismiss_job(id) {
            self.jobs_changed();
            cx.notify();
        }
    }

    pub(super) fn cancel_job(&mut self, id: u64, cx: &mut Context<Self>) {
        self.browser.cancel_job(id);
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
