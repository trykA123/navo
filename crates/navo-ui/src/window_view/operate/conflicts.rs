//! The copy and move dialog's conflict row: "N already there" on its own summary line and one
//! choice for all of them (Skip, Overwrite older, Keep both, Overwrite all); a pick rebuilds the
//! plan.

use navo_core::{ConflictPolicy, OpKind, OperationPlan, Resolution};
use navo_l10n::{t, tr};

use super::super::NavoWindow;
use crate::browser::{KeepOld, PendingOp};
use crate::confirm_dialog::{ConfirmDialog, OptionItem, Options};
use crate::format;
use crate::prelude::*;

/// One choice of the row: the policy it plans with, its element id and its label key.
struct ConflictChoice {
    policy: ConflictPolicy,
    id: &'static str,
    label: &'static str,
}

/// Every choice, in the order the row shows them.
const CHOICES: [ConflictChoice; 4] = [
    ConflictChoice {
        policy: ConflictPolicy::Skip,
        id: "conflict-skip",
        label: "conflict-skip",
    },
    ConflictChoice {
        policy: ConflictPolicy::OverwriteOlder,
        id: "conflict-overwrite-older",
        label: "conflict-overwrite-older",
    },
    ConflictChoice {
        policy: ConflictPolicy::Rename,
        id: "conflict-keep-both",
        label: "conflict-keep-both",
    },
    ConflictChoice {
        policy: ConflictPolicy::OverwriteAll,
        id: "conflict-overwrite-all",
        label: "conflict-overwrite-all",
    },
];

/// The choices shown: all four, or only Skip and Keep both when Undo could not restore an
/// overwritten file (`undoes_overwrites` false).
fn shown(undoes_overwrites: bool) -> Vec<&'static ConflictChoice> {
    CHOICES
        .iter()
        .filter(|choice| {
            undoes_overwrites
                || matches!(choice.policy, ConflictPolicy::Skip | ConflictPolicy::Rename)
        })
        .collect()
}

/// The index among `choices` of the choice `shown_policy` highlights.
fn picked(choices: &[&ConflictChoice], policy: &ConflictPolicy, plan: &OperationPlan) -> usize {
    let policy = shown_policy(policy, plan);
    choices
        .iter()
        .position(|choice| choice.policy == policy)
        .unwrap_or(0)
}

/// The choice the row highlights for `policy` on `plan`. A plan that asks shows what it really
/// does: Keep both when it renames the clashes (a paste into the same folder), else Skip.
pub(super) fn shown_policy(policy: &ConflictPolicy, plan: &OperationPlan) -> ConflictPolicy {
    let renames = || {
        plan.conflicts
            .iter()
            .any(|c| matches!(c.resolution, Resolution::Rename(_)))
    };
    match policy {
        ConflictPolicy::Ask if renames() => ConflictPolicy::Rename,
        ConflictPolicy::Ask => ConflictPolicy::Skip,
        other => other.clone(),
    }
}

/// True when the dialog for `pending` offers the row: a copy or move whose plan found names
/// already at the target, outside an undo offer and the "run without old versions" question.
fn offers_row(pending: &PendingOp) -> bool {
    matches!(pending.plan.kind, OpKind::Copy | OpKind::Move)
        && !pending.plan.conflicts.is_empty()
        && pending.undo.is_none()
        && pending.keep_old == KeepOld::AsSet
}

/// Counts `n` and formats it for a label.
fn counted(key: &str, n: usize) -> Option<String> {
    let count = u64::try_from(n).unwrap_or(u64::MAX);
    (n > 0).then(|| t!(key, count = count, shown = format::bytes(count)))
}

/// Puts the conflict line for `plan` on its own line below `summary`, so it is never cut.
pub(super) fn below(summary: String, plan: &OperationPlan) -> String {
    match self::summary(plan) {
        Some(conflicts) => format!("{summary}\n{conflicts}"),
        None => summary,
    }
}

/// The summary part for `plan`'s conflicts: "3 items already there will be skipped", or what
/// happens to them ("…: 1 overwritten, 2 skipped"); `None` when nothing is already there.
fn summary(plan: &OperationPlan) -> Option<String> {
    let conflicts = &plan.conflicts;
    if conflicts.is_empty() {
        return None;
    }
    let items = super::items(conflicts.len());
    let count = |wanted: fn(&Resolution) -> bool| {
        conflicts.iter().filter(|c| wanted(&c.resolution)).count()
    };
    let overwritten = count(|r| matches!(r, Resolution::Overwrite));
    let renamed = count(|r| matches!(r, Resolution::Rename(_)));
    let skipped = conflicts.len() - overwritten - renamed;
    if skipped == conflicts.len() {
        return Some(t!("conflict-all-skipped", items = items.as_str()));
    }
    let outcome = [
        counted("conflict-overwritten", overwritten),
        counted("conflict-renamed", renamed),
        counted("conflict-skipped", skipped),
    ];
    let outcome = outcome.into_iter().flatten().collect::<Vec<_>>().join(", ");
    Some(t!(
        "conflict-outcome",
        items = items.as_str(),
        outcome = outcome
    ))
}

impl NavoWindow {
    /// Adds the conflict row to `dialog` when `pending` offers it; a pick goes to the browser,
    /// which rebuilds the plan with that choice.
    pub(super) fn conflict_row(
        &self,
        dialog: ConfirmDialog,
        pending: &PendingOp,
        cx: &Context<Self>,
    ) -> ConfirmDialog {
        if !offers_row(pending) {
            return dialog;
        }
        let choices = shown(self.browser.undoes_overwrites());
        let options = Options {
            label: tr("conflict-choice-label").into(),
            items: choices
                .iter()
                .map(|choice| OptionItem {
                    id: choice.id.into(),
                    label: tr(choice.label).into(),
                })
                .collect(),
            picked: picked(&choices, &pending.replan.policy, &pending.plan),
        };
        let policies: Vec<ConflictPolicy> = choices.iter().map(|c| c.policy.clone()).collect();
        let view = cx.weak_entity();
        dialog.options(options, move |index, _, cx| {
            let Some(policy) = policies.get(index).cloned() else {
                return;
            };
            let _ = view.update(cx, |view, cx| {
                view.browser.pick_policy(policy);
                cx.notify();
            });
        })
    }
}
