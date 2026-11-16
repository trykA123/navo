use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::Instant;

use crate::content::{self, CheckOptions, ContentProgress, Side};
use crate::model::{CancelToken, ContentClass, Generation, Reason, Row, SideSource, Status};
use crate::queue::{CheckEvent, Job, Queue, Settlement};

use super::shared::Shared;
use super::{ComparePhase, PhaseTelemetry, SessionOptions};

mod threads;
mod walk;

#[cfg(test)]
pub(super) type SettleHook = Arc<dyn Fn(bool, u64) + Send + Sync>;

pub(super) struct Run {
    pub(super) left: Arc<dyn SideSource>,
    pub(super) right: Arc<dyn SideSource>,
    pub(super) options: SessionOptions,
    pub(super) generation: Generation,
    pub(super) cancel: CancelToken,
    pub(super) shared: Arc<Shared>,
    pub(super) queue: Arc<Queue>,
    pub(super) next_row_id: Arc<AtomicU64>,
    #[cfg(test)]
    pub(super) settle_hook: Option<SettleHook>,
}

struct Context {
    left: Arc<dyn SideSource>,
    right: Arc<dyn SideSource>,
    options: SessionOptions,
    generation: Generation,
    cancel: CancelToken,
    shared: Arc<Shared>,
    queue: Arc<Queue>,
    next_row_id: Arc<AtomicU64>,
    content_done: AtomicU64,
    #[cfg(test)]
    settle_hook: Option<SettleHook>,
}

pub(super) fn spawn(run: Run) -> Vec<JoinHandle<()>> {
    let workers = run.queue.workers();
    let context = Arc::new(Context {
        left: run.left,
        right: run.right,
        options: run.options,
        generation: run.generation,
        cancel: run.cancel,
        shared: run.shared,
        queue: run.queue,
        next_row_id: run.next_row_id,
        content_done: AtomicU64::new(0),
        #[cfg(test)]
        settle_hook: run.settle_hook,
    });
    let mut handles = Vec::with_capacity(workers + 1);
    let mut started = true;
    for _ in 0..workers {
        let inner = Arc::clone(&context);
        started = started && threads::start(&mut handles, move || worker(&inner));
    }
    let walker = Arc::clone(&context);
    if !(started && threads::start(&mut handles, move || threads::walk_guarded(&walker))) {
        threads::stop(&context);
    }
    handles
}

#[cfg(test)]
fn call_settle_hook(context: &Context, after_complete: bool, id: u64) {
    if let Some(hook) = &context.settle_hook {
        hook(after_complete, id);
    }
}

fn worker(context: &Context) {
    loop {
        if context.cancel.is_cancelled() {
            break;
        }
        let Some(event) = context.queue.next() else {
            break;
        };
        match event {
            CheckEvent::Job(job) => {
                if !run_job(context, job) {
                    break;
                }
            }
            CheckEvent::Stopped(id) => {
                let Some((mut row, parent)) = context.shared.take_row(id) else {
                    continue;
                };
                row.raw_status = Status::NotChecked(Reason::Stopped);
                row.display_status = row.raw_status;
                row.content = ContentClass::Unknown;
                record_content_check(context, true);
                if !context.shared.emit_final(context.generation, row, parent) {
                    break;
                }
            }
        }
    }
}

fn run_job(context: &Context, job: Job) -> bool {
    let Some((row, parent)) = context.shared.take_row(job.id) else {
        context.queue.complete(job.id);
        return false;
    };
    if context.cancel.is_cancelled() {
        context.queue.complete(job.id);
        return false;
    }
    let checked = catch_unwind(AssertUnwindSafe(|| check_row(context, row.clone(), &job)));
    let mut row = match checked {
        Ok(row) => row,
        Err(_) => {
            let mut row = row;
            row.raw_status = Status::Unavailable(Reason::WorkerPanicked);
            row.display_status = row.raw_status;
            row.content = ContentClass::Unknown;
            row
        }
    };
    #[cfg(test)]
    call_settle_hook(context, false, job.id);
    let settlement = context.queue.complete(job.id);
    #[cfg(test)]
    call_settle_hook(context, true, job.id);
    if settlement == Settlement::Stopped {
        row.raw_status = Status::NotChecked(Reason::Stopped);
        row.display_status = row.raw_status;
        row.content = ContentClass::Unknown;
    }
    record_content_check(context, true);
    context.shared.emit_final(context.generation, row, parent)
}

fn record_content_check(context: &Context, completed: bool) {
    let done = if completed {
        context
            .content_done
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1)
    } else {
        context.content_done.load(Ordering::Relaxed)
    };
    let remaining = u64::try_from(context.queue.remaining()).unwrap_or(u64::MAX);
    context.shared.record_phase(
        context.generation,
        PhaseTelemetry {
            phase: ComparePhase::ContentCheck,
            done,
            total: Some(done.saturating_add(remaining)),
        },
    );
}

fn check_row(context: &Context, mut row: Row, job: &Job) -> Row {
    let (Some(left_entry), Some(right_entry)) = (row.left.as_deref(), row.right.as_deref()) else {
        return row;
    };
    let options = CheckOptions {
        cache: &*context.options.cache,
        cancel: &job.cancel,
        read_hint: job.read_hint,
        known_same: false,
    };
    let mut report = |progress: ContentProgress| {
        if let Some(progress) =
            context
                .queue
                .progress(job.id, progress.bytes_done, progress.bytes_total)
        {
            record_content_check(context, false);
            context.shared.emit_progress(context.generation, progress);
        }
    };
    let started = Instant::now();
    let result = content::check(
        Side {
            entry: left_entry,
            source: &*context.left,
        },
        Side {
            entry: right_entry,
            source: &*context.right,
        },
        &options,
        &mut report,
    );
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    context
        .shared
        .record_slow(context.generation, left_entry.rel.clone(), elapsed_ms);
    row.raw_status = result.status;
    row.display_status = result.status;
    row.content = result.content;
    row
}
