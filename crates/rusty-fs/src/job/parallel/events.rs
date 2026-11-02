use std::sync::mpsc::RecvTimeoutError;

use super::*;
use crate::job::copy::{Copied, place};

impl Runner<'_, '_, '_> {
    pub(super) fn receive(&mut self) {
        let event = match self.marks.due_in() {
            Some(wait) => self.pool.events.recv_timeout(wait),
            None => self
                .pool
                .events
                .recv()
                .map_err(|_| RecvTimeoutError::Disconnected),
        };
        match event {
            Ok(Event::Created { seq, identity, ack }) => {
                let result = self
                    .ctx
                    .log
                    .created_temporary((seq, identity), self.ctx.ops);
                let _ = ack.send(result);
            }
            Ok(Event::Bytes { seq, bytes }) => {
                if let Some((step, previous)) = self.in_flight.get_mut(&seq) {
                    let planned = counted(self.job.plan.kind, step).0;
                    self.progress
                        .adjust_total(planned.max(*previous), planned.max(bytes));
                    *previous = bytes;
                }
                self.report();
            }
            Ok(Event::Finished { seq, result }) => {
                self.finish_copy(seq, result);
            }
            Ok(Event::Removed { seq, result }) => {
                if let Some((step, _)) = self.in_flight.remove(&seq) {
                    self.settle(seq, step, result.map(|outcome| (outcome, 0)));
                }
            }
            Err(RecvTimeoutError::Timeout) => self.flush_marks(),
            Err(RecvTimeoutError::Disconnected) => {
                let lost: Vec<u32> = self.in_flight.drain().map(|(seq, _)| seq).collect();
                for seq in lost {
                    self.fail(seq, FsError::new(FsErrorKind::Unknown));
                }
            }
        }
    }

    fn finish_copy(&mut self, seq: u32, result: Result<Copied, FsError>) {
        let Some((step, streamed)) = self.in_flight.remove(&seq) else {
            return;
        };
        let planned = counted(self.job.plan.kind, step).0;
        let written = result
            .as_ref()
            .ok()
            .and_then(Copied::bytes)
            .unwrap_or(planned);
        if result.is_ok() {
            self.progress.adjust_total(planned.max(streamed), written);
        }
        let result = result
            .and_then(|copied| self.place(step, copied))
            .map(|outcome| (outcome, written));
        self.settle(seq, step, result);
    }

    fn place(&mut self, step: &Step, copied: Copied) -> Result<StepOutcome, FsError> {
        let Step::CopyFile { target, .. } = step else {
            return Err(FsError::new(FsErrorKind::Unknown));
        };
        place(self.job.aside, self.ctx, copied, target)
    }

    pub(super) fn settle(
        &mut self,
        seq: u32,
        step: &Step,
        result: Result<(StepOutcome, u64), FsError>,
    ) {
        match result {
            Ok((outcome, bytes)) => {
                note_kept(step, outcome, &mut self.kept);
                if let Err((seq, error)) = self.marks.record(self.ctx.log, (seq, step), outcome) {
                    self.fail(seq, error);
                    return;
                }
                self.count(step, bytes);
                self.report();
            }
            Err(error)
                if error.kind == FsErrorKind::Cancelled || self.ctx.cancel.is_cancelled() =>
            {
                self.cancelled = true;
            }
            Err(error) => self.settle_failure(seq, error),
        }
    }

    fn settle_failure(&mut self, seq: u32, error: FsError) {
        if error.kind == FsErrorKind::Cancelled || self.ctx.cancel.is_cancelled() {
            self.cancelled = true;
            return;
        }
        self.marks.failed(self.ctx.log, seq);
        if self.job.plan.kind == OpKind::Recycle {
            self.note_failure(seq, error);
            return;
        }
        self.fail(seq, error);
    }
}
