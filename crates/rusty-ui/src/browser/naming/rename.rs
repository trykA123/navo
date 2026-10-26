use rusty_core::rename::check::{Existing, ExistingName, Fold, Row};
use rusty_core::rename::plan_batch;
use rusty_core::{NameError, OpKind, check_name};
use rusty_fs::JobOutcome;

use super::{Browser, NameProblem};

impl Browser {
    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), NameProblem> {
        if from == to {
            return Err(NameProblem::Unchanged);
        }
        check_name(to).map_err(NameProblem::Invalid)?;
        if self.renaming() {
            return Err(NameProblem::Busy);
        }
        if self.taken(to, Some(from)) {
            return Err(NameProblem::Exists);
        }
        let parent = self
            .here(from)
            .and_then(|source| source.parent())
            .ok_or(NameProblem::Invalid(NameError::Empty))?;
        let tab = self.state.active_tab().id();
        let existing = Existing::new(
            self.full_names(tab)
                .into_iter()
                .map(|name| ExistingName {
                    parent: parent.clone(),
                    name,
                })
                .collect(),
        );
        let row = Row {
            parent,
            old: from.to_owned(),
            new: to.to_owned(),
            excluded: false,
        };
        let seed = u32::try_from(self.next_op).unwrap_or(u32::MAX);
        let plan = plan_batch(&[row], &existing, Fold::Insensitive, seed)
            .plan
            .ok_or(NameProblem::Exists)?;
        if !self.rename_batch(plan, Some(to)) {
            return Err(NameProblem::Busy);
        }
        Ok(())
    }

    pub(in crate::browser) fn note_rename_error(
        &mut self,
        id: u64,
        outcome: &Result<JobOutcome, String>,
    ) {
        let Ok(JobOutcome::Failed { error, .. }) = outcome else {
            return;
        };
        let Some(job) = self
            .jobs
            .iter()
            .find(|job| job.id == id && job.kind == OpKind::Rename)
        else {
            return;
        };
        if self
            .state
            .tab(job.source_tab)
            .is_some_and(|tab| Some(tab.location()) == job.from.as_ref())
        {
            self.late_errors.insert(job.source_tab, error.clone());
        }
    }
}
