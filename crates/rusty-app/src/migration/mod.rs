mod dialog;
mod fs;
mod journal;
mod layout;
mod lenient;
mod probe;
mod start;
mod steps;

use std::io;
use std::path::Path;

use rusty_store::StoreDirs;

pub(crate) use dialog::Dialogs;
pub(crate) use fs::{MigrationFs, RealFs};
pub(crate) use journal::Phase;
use journal::{Ending, Record};
use layout::Plan;
pub(crate) use lenient::LenientSync;
pub(crate) use start::{Report, Settled, run as start};

const TEST_EXIT_CODE: i32 = 86;

#[derive(Debug)]
pub(crate) enum Outcome {
    Done,
    Resumed,
    NothingToMigrate(Reason),
    Blocked,
    Failed(Failure),
    MigratedRetirePending(Option<Failure>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    NoLegacyProfile,
    NavoProfileExists,
    AlreadyMigrated,
    StartedFresh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    Journal,
    Detach,
    Stage,
    Commit,
    Retire,
    Finish,
    Rollback,
}

#[derive(Debug)]
pub(crate) struct Failure {
    pub(crate) step: Step,
    pub(crate) error: io::Error,
    committed: bool,
}

impl Failure {
    fn new(step: Step, error: io::Error, committed: bool) -> Self {
        Self {
            step,
            error,
            committed,
        }
    }

    pub(crate) fn can_start_fresh(&self) -> bool {
        !self.committed
    }

    pub(crate) fn is_in_use(&self) -> bool {
        steps::is_in_use(
            &self.error,
            matches!(self.step, Step::Commit | Step::Retire),
        )
    }
}

pub(crate) struct Migration<'a, F: MigrationFs> {
    fs: &'a F,
    old: StoreDirs,
    new: StoreDirs,
    exit_after: Option<Phase>,
}

impl<'a, F: MigrationFs> Migration<'a, F> {
    pub(crate) fn user(fs: &'a F, old: StoreDirs, new: StoreDirs) -> Self {
        Self {
            fs,
            old,
            new,
            exit_after: None,
        }
    }

    pub(crate) fn in_profile(fs: &'a F, dirs: StoreDirs) -> Self {
        Self::user(fs, dirs.clone(), dirs)
    }

    #[must_use]
    pub(crate) fn exit_after(self, phase: Option<Phase>) -> Self {
        Self {
            exit_after: phase,
            ..self
        }
    }

    pub(crate) fn run(&self, is_running: impl FnOnce() -> bool) -> Outcome {
        let plan = match Plan::new(&self.old, &self.new) {
            Ok(plan) => plan,
            Err(error) => return Outcome::Failed(Failure::new(Step::Journal, error, false)),
        };
        let record = match self.read_journal(&plan) {
            Ok(record) => record,
            Err(failure) => return Outcome::Failed(failure),
        };
        if record.is_none()
            && let Err(reason) = self.decide(&plan)
        {
            return Outcome::NothingToMigrate(reason);
        }
        if is_running() {
            return match record {
                Some(record) if record.phase >= Phase::Committed => {
                    Outcome::MigratedRetirePending(None)
                }
                _ => Outcome::Blocked,
            };
        }
        match record {
            Some(record) => self.resume(&record, &plan.journal),
            None => self.migrate(&plan),
        }
    }

    pub(crate) fn start_fresh(&self) -> Result<(), Failure> {
        let plan = Plan::new(&self.old, &self.new)
            .map_err(|error| Failure::new(Step::Journal, error, false))?;
        let rollback = |error| Failure::new(Step::Rollback, error, false);
        let refused = || {
            let error = io::Error::other("the Navo profile already holds the copied files");
            Failure::new(Step::Rollback, error, true)
        };
        match self.read_journal(&plan)? {
            Some(record) => {
                let recorded = recorded_plan(&record, &plan.journal).map_err(rollback)?;
                if !can_roll_back(self.fs, &record, &recorded) {
                    return Err(refused());
                }
                steps::rollback(self.fs, &recorded).map_err(rollback)?;
            }
            None if journal::read_marker(self.fs, &plan.marker) == Some(Ending::Migrated) => {
                return Err(refused());
            }
            None => journal::remove(self.fs, &plan.journal).map_err(rollback)?,
        }
        journal::write_marker(self.fs, &plan.marker, Ending::StartedFresh)
            .map_err(|error| Failure::new(Step::Finish, error, false))
    }

    fn read_journal(&self, plan: &Plan) -> Result<Option<Record>, Failure> {
        let fail = |error| Failure::new(Step::Journal, error, true);
        if let Ok(record) = journal::read(self.fs, &plan.journal) {
            return Ok(record);
        }
        journal::set_aside(self.fs, &plan.journal).map_err(fail)?;
        if journal::read_marker(self.fs, &plan.marker).is_some() {
            journal::remove(self.fs, &plan.journal).map_err(fail)?;
            return Ok(None);
        }
        let record = Record::new(steps::infer_phase(self.fs, plan), &self.old, &self.new);
        journal::write(self.fs, &plan.journal, &record).map_err(fail)?;
        Ok(Some(record))
    }

    fn decide(&self, plan: &Plan) -> Result<(), Reason> {
        match journal::read_marker(self.fs, &plan.marker) {
            Some(Ending::Migrated) => return Err(Reason::AlreadyMigrated),
            Some(Ending::StartedFresh) => return Err(Reason::StartedFresh),
            None => {}
        }
        if steps::holds_navo_data(self.fs, plan) {
            return Err(Reason::NavoProfileExists);
        }
        if !steps::holds_legacy_data(self.fs, plan) {
            return Err(Reason::NoLegacyProfile);
        }
        Ok(())
    }

    fn migrate(&self, plan: &Plan) -> Outcome {
        let record = Record::new(Phase::Detached, &self.old, &self.new);
        if let Err(error) = journal::write(self.fs, &plan.journal, &record) {
            return self.abandon(plan, Failure::new(Step::Journal, error, false));
        }
        if let Err(error) = steps::detach(self.fs, plan) {
            let blocked = steps::is_in_use(&error, false);
            let outcome = self.abandon(plan, Failure::new(Step::Detach, error, false));
            return match outcome {
                Outcome::Failed(failure) if blocked && failure.step == Step::Detach => {
                    Outcome::Blocked
                }
                other => other,
            };
        }
        self.reached(Phase::Detached);
        if let Err(error) = steps::stage(self.fs, plan) {
            return self.abandon(plan, Failure::new(Step::Stage, error, false));
        }
        if let Err(error) = journal::write(self.fs, &plan.journal, &record.at(Phase::Staged)) {
            return self.unstage(plan, &record, error);
        }
        self.reached(Phase::Staged);
        self.forward(plan, &record.at(Phase::Staged), Outcome::Done)
    }

    fn unstage(&self, plan: &Plan, record: &Record, error: io::Error) -> Outcome {
        match journal::write(self.fs, &plan.journal, record) {
            Ok(()) => self.abandon(plan, Failure::new(Step::Journal, error, false)),
            Err(_) => Outcome::Failed(Failure::new(Step::Journal, error, false)),
        }
    }

    fn abandon(&self, plan: &Plan, failure: Failure) -> Outcome {
        match steps::rollback(self.fs, plan) {
            Ok(()) => Outcome::Failed(failure),
            Err(error) => Outcome::Failed(Failure::new(Step::Rollback, error, false)),
        }
    }

    fn resume(&self, record: &Record, journal: &Path) -> Outcome {
        let plan = match recorded_plan(record, journal) {
            Ok(plan) => plan,
            Err(error) => return Outcome::Failed(Failure::new(Step::Journal, error, true)),
        };
        if record.phase != Phase::Detached {
            return self.forward(&plan, record, Outcome::Resumed);
        }
        if let Err(error) = steps::rollback(self.fs, &plan) {
            return Outcome::Failed(Failure::new(Step::Rollback, error, false));
        }
        let plan = match Plan::new(&self.old, &self.new) {
            Ok(plan) => plan,
            Err(error) => return Outcome::Failed(Failure::new(Step::Journal, error, false)),
        };
        match self.decide(&plan) {
            Err(reason) => Outcome::NothingToMigrate(reason),
            Ok(()) => match self.migrate(&plan) {
                Outcome::Done => Outcome::Resumed,
                other => other,
            },
        }
    }

    fn forward(&self, plan: &Plan, record: &Record, done: Outcome) -> Outcome {
        if record.phase == Phase::Staged {
            if let Err(error) = steps::commit(self.fs, plan) {
                let started = steps::commit_started(self.fs, plan);
                return Outcome::Failed(Failure::new(Step::Commit, error, started));
            }
            let committed = record.at(Phase::Committed);
            if let Err(error) = journal::write(self.fs, &plan.journal, &committed) {
                return Outcome::Failed(Failure::new(Step::Journal, error, true));
            }
            self.reached(Phase::Committed);
        }
        match self.finish(plan, record) {
            Ok(()) => done,
            Err(failure) => Outcome::MigratedRetirePending(Some(failure)),
        }
    }

    fn finish(&self, plan: &Plan, record: &Record) -> Result<(), Failure> {
        let fail = |step| move |error| Failure::new(step, error, true);
        if record.phase != Phase::Retired {
            steps::retire(self.fs, plan).map_err(fail(Step::Retire))?;
            journal::write(self.fs, &plan.journal, &record.at(Phase::Retired))
                .map_err(fail(Step::Journal))?;
            self.reached(Phase::Retired);
        }
        journal::write_marker(self.fs, &plan.marker, Ending::Migrated)
            .and_then(|()| journal::remove(self.fs, &plan.journal))
            .map_err(fail(Step::Finish))
    }

    fn reached(&self, phase: Phase) {
        if self.exit_after == Some(phase) {
            std::process::exit(TEST_EXIT_CODE);
        }
    }
}

fn can_roll_back(fs: &impl MigrationFs, record: &Record, plan: &Plan) -> bool {
    match record.phase {
        Phase::Detached => true,
        Phase::Staged => !steps::commit_started(fs, plan),
        Phase::Committed | Phase::Retired => false,
    }
}

fn recorded_plan(record: &Record, journal: &Path) -> io::Result<Plan> {
    let mut plan = Plan::new(&(&record.old).into(), &(&record.new).into())?;
    journal.clone_into(&mut plan.journal);
    Ok(plan)
}

pub(crate) fn test_exit_phase(var: impl Fn(&str) -> Option<String>) -> Option<Phase> {
    if var("NAVO_CRASH_TEST").as_deref() != Some("1") {
        return None;
    }
    var("NAVO_MIGRATION_TEST_EXIT")?.parse().ok()
}

#[cfg(test)]
mod tests;
