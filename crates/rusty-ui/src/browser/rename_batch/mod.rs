use std::sync::Arc;

use rusty_core::{Location, OpKind, OperationPlan, Step, TabId};

use super::Browser;
use super::operate::{PendingOp, Run};

mod hooks;
mod run;

pub use hooks::{
    DeletePreset, LastRename, ListPresets, NamedPreset, RenameHooks, RenameLogStatus,
    RenamePresetList, RenameReport, RetireUndo, SavePreset, UndoKind, UndoLogFactory, UndoOffer,
    WriteRenameLog,
};
pub(super) use run::run_batch;
pub use run::run_undo;

#[derive(Default)]
pub(super) struct RenameSettings {
    pub hooks: Option<RenameHooks>,
    pub logging: bool,
    generation: u64,
    dialog_generation: u64,
}

impl Browser {
    pub(crate) fn undo_generation(&self) -> u64 {
        self.hooks.rename.generation
    }

    pub(crate) fn invalidate_undo(&mut self) -> u64 {
        self.hooks.rename.generation += 1;
        self.undo_generation()
    }

    pub(crate) fn dialog_generation(&self) -> u64 {
        self.hooks.rename.dialog_generation
    }

    pub(crate) fn dialog_opened(&mut self) {
        self.hooks.rename.dialog_generation += 1;
        self.invalidate_undo();
    }

    pub fn set_rename_logging(&mut self, enabled: bool) {
        self.hooks.rename.logging = enabled;
    }

    pub fn set_rename_hooks(&mut self, hooks: Option<RenameHooks>) {
        self.hooks.rename.hooks = hooks;
    }

    #[must_use]
    pub fn rename_hooks(&self) -> Option<&RenameHooks> {
        self.hooks.rename.hooks.as_ref()
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
        let log = self
            .hooks
            .rename
            .hooks
            .as_ref()
            .filter(|_| self.hooks.rename.logging)
            .and_then(|hooks| hooks.log.clone());
        self.start_job(pending, Run::Rename { log }, first_new.map(str::to_owned));
        true
    }

    pub fn undo_rename(&mut self, offer: UndoOffer) -> bool {
        match self.undo_pending(offer) {
            Some(pending) => self.start_undo(pending),
            None => false,
        }
    }

    pub fn ask_undo(&mut self, offer: UndoOffer) -> bool {
        let Some(pending) = self.undo_pending(offer) else {
            return false;
        };
        self.dialog_opened();
        self.pending = Some(pending);
        true
    }

    pub(super) fn start_undo(&mut self, pending: PendingOp) -> bool {
        let (Some(offer), Some(hooks)) = (pending.undo.clone(), &self.hooks.rename.hooks) else {
            return false;
        };
        if self.renaming() {
            return false;
        }
        let (last, log) = (hooks.last.clone(), hooks.undo_log.clone());
        let log = Arc::new(move |offer: &UndoOffer| {
            if !last()?.is_some_and(|latest| latest.job == offer.job) {
                return Err(rusty_l10n::tr("undo-nothing"));
            }
            log(offer)
        });
        self.start_job(pending, Run::Undo { offer, log }, None);
        true
    }

    fn undo_pending(&mut self, offer: UndoOffer) -> Option<PendingOp> {
        self.hooks.rename.hooks.as_ref()?;
        let items = offer.items();
        if (items == 0 && !offer.restore_pending) || self.renaming() {
            return None;
        }
        let mut plan = offer.plan.clone();
        plan.totals.files = u64::try_from(items).unwrap_or(u64::MAX);
        let sources = offer.item_targets().cloned().collect();
        let mut pending = self.pending_for(sources, plan);
        pending.undo = Some(offer);
        Some(pending)
    }

    pub fn renaming(&self) -> bool {
        self.jobs
            .iter()
            .any(|job| (job.kind == OpKind::Rename || job.undo) && job.status.is_active())
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
            retain_deletes: None,
            cannot_retain: false,
            source_tab: self.state.active_tab().id(),
            undo: None,
            slow_volumes: None,
            structure: None,
        }
    }
}
