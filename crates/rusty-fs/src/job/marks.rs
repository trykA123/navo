use std::time::{Duration, Instant};

use rusty_core::{FsError, FsErrorKind, OpKind, Step};

use super::{JobLog, StepOutcome};

const BATCH: usize = 512;
const EVERY: Duration = Duration::from_millis(250);
pub(super) const WHOLE_JOB: u32 = u32::MAX;

pub(super) struct Marks {
    batches: bool,
    waiting: Vec<(u32, StepOutcome)>,
    since: Instant,
}

impl Marks {
    pub(super) fn new(kind: OpKind) -> Self {
        Self {
            batches: kind == OpKind::Delete,
            waiting: Vec::new(),
            since: Instant::now(),
        }
    }

    pub(super) fn record(
        &mut self,
        log: &mut dyn JobLog,
        (seq, step): (u32, &Step),
        outcome: StepOutcome,
    ) -> Result<(), (u32, FsError)> {
        if !self.batches || outcome == StepOutcome::Failed || !is_removal(step) {
            self.flush(log)?;
            return log.step(seq, outcome).map_err(|error| (seq, error));
        }
        if self.waiting.is_empty() {
            self.since = Instant::now();
        }
        self.waiting.push((seq, outcome));
        if self.waiting.len() >= BATCH || self.since.elapsed() >= EVERY {
            self.flush(log)?;
        }
        Ok(())
    }

    pub(super) fn due_in(&self) -> Option<Duration> {
        (!self.waiting.is_empty()).then(|| EVERY.saturating_sub(self.since.elapsed()))
    }

    pub(super) fn flush(&mut self, log: &mut dyn JobLog) -> Result<(), (u32, FsError)> {
        if self.waiting.is_empty() {
            return Ok(());
        }
        let written = log.steps(&self.waiting);
        self.waiting.clear();
        written.map_err(|error| {
            let error = FsError {
                kind: FsErrorKind::JournalWriteFailed,
                location: None,
                ..error
            };
            (WHOLE_JOB, error)
        })
    }

    pub(super) fn failed(&mut self, log: &mut dyn JobLog, seq: u32) {
        let _ = self.flush(log);
        let _ = log.step(seq, StepOutcome::Failed);
    }
}

pub(super) fn is_removal(step: &Step) -> bool {
    matches!(
        step,
        Step::RemoveFile { .. } | Step::RemoveLink { .. } | Step::RemoveFolder { .. }
    )
}
