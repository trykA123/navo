use std::sync::Arc;

use rusty_core::{FsError, OperationPlan};
use rusty_fs::{HeldFile, JobLog, JobOutcome, ProviderOps, StepOutcome};
use rusty_store::StoreDirs;
use rusty_ui::browser::{LogFactory, LoggedOperation, OpLog, WriteFactory};

use super::profile;

pub(crate) fn guard_factory(dirs: Option<StoreDirs>) -> LogFactory {
    Arc::new(move |plan| {
        guard(plan, dirs.as_ref())?;
        Ok(Box::new(GuardLog { dirs: dirs.clone() }) as Box<dyn OpLog>)
    })
}

pub(crate) fn guard_write(dirs: Option<StoreDirs>) -> WriteFactory {
    match dirs {
        Some(dirs) => profile::write_factory(dirs, false),
        None => Arc::new(|target| {
            rusty_fs::mutation_location(target)?;
            Ok(Box::new(()) as Box<dyn Send>)
        }),
    }
}

fn guard(plan: &OperationPlan, dirs: Option<&StoreDirs>) -> Result<(), String> {
    match dirs {
        Some(dirs) => profile::guard(plan, dirs, false),
        None => profile::mutated_paths(plan).map(drop),
    }
}

struct GuardLog {
    dirs: Option<StoreDirs>,
}

impl JobLog for GuardLog {
    fn step(&mut self, _: u32, _: StepOutcome) -> Result<(), FsError> {
        Ok(())
    }

    fn undone(&mut self, _: u32, _: StepOutcome) -> Result<(), FsError> {
        Ok(())
    }

    fn journal_held(&mut self, _: &HeldFile) -> Result<bool, FsError> {
        Ok(false)
    }
}

impl OpLog for GuardLog {
    fn check(&mut self, operation: LoggedOperation<'_>) -> Result<(), String> {
        guard(operation.plan, self.dirs.as_ref())?;
        rusty_fs::check_relocation(operation.plan, operation.ops, operation.cancel)
            .map_err(|error| error.message())
    }

    fn finish(&mut self, _: &JobOutcome, _: &dyn ProviderOps) -> Result<(), String> {
        Ok(())
    }
}
