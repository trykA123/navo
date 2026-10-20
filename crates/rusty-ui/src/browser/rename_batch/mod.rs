use std::sync::Arc;

use rusty_core::{Location, OpKind, OperationPlan, Step, TabId};

use super::operate::{PendingOp, Run};
use super::{Browser, JobStatus};

mod hooks;
mod run;

pub use hooks::{
    DeletePreset, LastRename, ListPresets, NamedPreset, RenameHooks, RenamePresetList,
    RenameReport, SavePreset, UndoLogFactory, UndoOffer,
};
pub(super) use run::{run_batch, run_undo};

impl Browser {
    pub fn set_rename_hooks(&mut self, hooks: RenameHooks) {
        self.rename_hooks = Some(hooks);
    }

    #[must_use]
    pub fn rename_hooks(&self) -> Option<&RenameHooks> {
        self.rename_hooks.as_ref()
    }

    #[must_use]
    pub fn full_names(&self, tab: TabId) -> Vec<String> {
        let listing = self
            .full
            .get(&tab)
            .map(|full| &full.listing)
            .or_else(|| self.state.tab(tab).and_then(|t| t.listing()));
        listing
            .map(|listing| listing.entries().iter().map(|e| e.name.clone()).collect())
            .unwrap_or_default()
    }

    pub fn rename_batch(&mut self, plan: OperationPlan, first_new: Option<&str>) -> bool {
        let sources: Vec<_> = plan
            .steps
            .iter()
            .filter(|step| hooks::is_item(step))
            .filter_map(|step| match step {
                Step::Rename { source, .. } => Some(source.clone()),
                _ => None,
            })
            .collect();
        if plan.kind != OpKind::Rename || sources.is_empty() || self.renaming() {
            return false;
        }
        let pending = self.pending_for(sources, plan);
        self.start_job(pending, Run::Rename, first_new.map(str::to_owned));
        true
    }

    pub fn undo_rename(&mut self, offer: UndoOffer) -> bool {
        let Some(hooks) = &self.rename_hooks else {
            return false;
        };
        let items = offer.items();
        if (items == 0 && !offer.restore_pending) || self.renaming() {
            return false;
        }
        let log = Arc::clone(&hooks.undo_log);
        let mut plan = offer.plan.clone();
        plan.totals.files = u64::try_from(items).unwrap_or(u64::MAX);
        let sources = offer.item_targets().cloned().collect();
        let pending = self.pending_for(sources, plan);
        self.start_job(pending, Run::Undo { offer, log }, None);
        true
    }

    pub fn renaming(&self) -> bool {
        self.jobs
            .iter()
            .any(|job| job.kind == OpKind::Rename && job.status == JobStatus::Running)
    }

    pub(super) fn land_on(&mut self, tab: TabId, folder: &Location, name: String) {
        if self.state.tab(tab).is_some_and(|t| t.location() == folder) {
            self.reveal_name(tab, &name);
            self.focus_after_load.insert(tab, name);
        }
    }

    fn pending_for(&mut self, sources: Vec<Location>, plan: OperationPlan) -> PendingOp {
        self.next_op += 1;
        PendingOp {
            id: self.next_op,
            sources,
            target: None,
            plan,
            source_tab: self.state.active_tab().id(),
        }
    }
}
