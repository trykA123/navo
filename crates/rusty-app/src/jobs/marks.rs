use rusty_core::{FsError, FsErrorKind};
use rusty_fs::StepOutcome;
use rusty_store::StepState;

use super::{JournalLog, lock};

fn step_state(outcome: StepOutcome) -> StepState {
    match outcome {
        StepOutcome::Done => StepState::Done,
        StepOutcome::Skipped => StepState::Skipped,
        StepOutcome::Failed => StepState::Failed,
    }
}

fn keeps_detail(log: &JournalLog) -> bool {
    log.renames || log.retained.is_some() || log.overwrites.is_some()
}

pub(super) fn mark(log: &JournalLog, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
    let mut journal = lock(&log.journal);
    let detail = if keeps_detail(log) {
        journal
            .step_detail(log.job, seq)
            .map_err(|_| FsError::new(FsErrorKind::Unknown))?
    } else {
        None
    };
    journal
        .mark_step(log.job, seq, step_state(outcome), detail.as_deref())
        .map_err(|_| FsError::new(FsErrorKind::Unknown))
}

pub(super) fn mark_all(log: &JournalLog, marks: &[(u32, StepOutcome)]) -> Result<(), FsError> {
    if keeps_detail(log) {
        return marks
            .iter()
            .try_for_each(|&(seq, outcome)| mark(log, seq, outcome));
    }
    let states: Vec<(u32, StepState)> = marks
        .iter()
        .map(|&(seq, outcome)| (seq, step_state(outcome)))
        .collect();
    lock(&log.journal)
        .mark_steps(log.job, &states)
        .map_err(|_| FsError::new(FsErrorKind::Unknown))
}
