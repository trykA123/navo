//! The large body's progress line and D17 step notices (V41a), shown beside the change counter.
//! Text only: the numbers change with each snapshot and nothing blinks or animates.

use navo_l10n::t;

use super::LargeShown;
use crate::compare_diff::{LargeStep, LargeStepReason};
use crate::prelude::*;

impl LargeShown {
    /// The progress line and step notices, joined with " · "; none when there is nothing to say.
    /// The progress line is hidden while next difference shows its own wait text.
    pub(super) fn notes(&self) -> Option<SharedString> {
        let mut parts = Vec::new();
        if let Some(progress) = self.progress.filter(|_| !self.finished() && !self.seeking) {
            let percent = progress.done.saturating_mul(100) / progress.total.max(1);
            let percent = percent.min(100);
            parts.push(t!("compare-screen-comparing-percent", percent = percent));
        }
        parts.extend(self.notices());
        (!parts.is_empty()).then(|| parts.join(" · ").into())
    }

    /// One notice per D17 step that cut detail: coarse blocks by budget, coarse blocks by a
    /// cap, and paged hunks. Each names the setting in plain text; V41c makes it a link.
    fn notices(&self) -> Vec<String> {
        let count = |wanted: &dyn Fn(LargeStepReason) -> bool, step: LargeStep| {
            (self.steps.iter())
                .filter(|view| view.step == step && wanted(view.reason))
                .count()
        };
        let budget = count(&|r| r == LargeStepReason::Budget, LargeStep::Coarse);
        let cap = count(&|r| r != LargeStepReason::Budget, LargeStep::Coarse);
        let paged = count(&|_| true, LargeStep::PagedHunks);
        let mut notes = Vec::new();
        if budget > 0 {
            notes.push(t!("compare-screen-notice-coarse-budget", sections = budget));
        }
        if cap > 0 {
            notes.push(t!("compare-screen-notice-coarse-cap", sections = cap));
        }
        if paged > 0 {
            notes.push(t!("compare-screen-notice-paged"));
        }
        notes
    }
}
