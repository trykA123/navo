//! `DiffHooks::copy_change` for the app: runs `copy_change` on its own thread and reports the end.
//! It holds the compare job factory and the local provider the copy writes through (V27).

use std::sync::Arc;
use std::thread;

use navo_compare::text::Direction;
use navo_fs::{CancelToken, ProviderOps};
use navo_ui::browser::LogFactory;
use navo_ui::compare_diff::{CopyChange, CopyDirection, CopyDone, CopyResult, CopySink};

use super::copy::{CopyContext, CopyRefusal, CopyRequest, copy_change};
use super::copy_many::plan::plan_sheet;
use super::copy_many::run_sheet;
use super::hooks::rules;
use super::sheet_hook::SheetSlot;

/// What a copy needs from the app that a pair does not carry.
#[derive(Clone)]
pub(super) struct CopyService {
    pub(super) factory: LogFactory,
    pub(super) ops: Arc<dyn ProviderOps>,
    /// The last plan sheet and its requests, kept for `run_sheet` (V31).
    pub(super) sheet: SheetSlot,
}

impl CopyService {
    /// A service over the job `factory` and the provider `ops`, with no plan kept yet.
    pub(super) fn new(factory: LogFactory, ops: Arc<dyn ProviderOps>) -> Self {
        Self {
            factory,
            ops,
            sheet: SheetSlot::default(),
        }
    }
}

/// The engine's direction for the UI's.
fn direction_of(direction: CopyDirection) -> Direction {
    match direction {
        CopyDirection::ToRight => Direction::LeftToRight,
        CopyDirection::ToLeft => Direction::RightToLeft,
    }
}

/// The request the engine runs for one change the UI showed.
pub(super) fn request_of(change: &CopyChange) -> CopyRequest {
    CopyRequest {
        pair: change.pair.clone(),
        left_bytes: change.left_bytes.clone(),
        right_bytes: change.right_bytes.clone(),
        digest: change.digest,
        direction: direction_of(change.direction),
        lines: change.lines.clone(),
    }
}

/// Sends a refusal when dropped before `finish`, so a thread that panics still ends the UI's
/// wait (a flag the UI sets while a copy runs would never clear otherwise).
struct EndGuard<T, R: FnOnce() -> T> {
    sink: Option<Arc<dyn Fn(T) + Send + Sync>>,
    refused: Option<R>,
}

impl<T, R: FnOnce() -> T> Drop for EndGuard<T, R> {
    /// Reports the refusal when the work ended without a result (a panic).
    fn drop(&mut self) {
        if let (Some(sink), Some(refused)) = (self.sink.take(), self.refused.take()) {
            sink(refused());
        }
    }
}

/// Runs `work` and reports its result through `sink`; if `work` panics, reports `refused()`
/// while the panic unwinds. Exactly one report is sent either way.
pub(super) fn guarded<T, R: FnOnce() -> T>(
    sink: Arc<dyn Fn(T) + Send + Sync>,
    refused: R,
    work: impl FnOnce() -> T,
) {
    let mut guard = EndGuard {
        sink: Some(sink),
        refused: Some(refused),
    };
    let result = work();
    if let Some(sink) = guard.sink.take() {
        sink(result);
    }
}

/// The report for a copy thread that stopped without an answer.
fn stopped(request: u64, direction: CopyDirection) -> CopyDone {
    let why = CopyRefusal::Failed("the copy stopped unexpectedly".to_owned());
    CopyDone {
        request,
        direction,
        result: CopyResult::Refused(why.text()),
    }
}

impl CopyService {
    /// Copy `change` on a new thread and report through `done`. A thread that cannot start
    /// reports a refusal, so the UI never waits for an answer that will not come.
    pub(super) fn start(&self, change: CopyChange, done: CopySink) {
        self.start_many(vec![change], done);
    }

    /// Copy `changes` of one pair as one job on a new thread and report through `done`; a
    /// lone change takes the one-change path. The report names the first change's pair.
    pub(super) fn start_many(&self, changes: Vec<CopyChange>, done: CopySink) {
        let service = self.clone();
        let Some(first) = changes.first() else {
            return;
        };
        let (request, direction) = (first.pair.id, first.direction);
        let report = done.clone();
        let spawned = thread::Builder::new()
            .name("compare-copy".to_owned())
            .spawn(move || {
                let refused = || stopped(request, direction);
                guarded(done, refused, || {
                    let requests: Vec<CopyRequest> = changes.iter().map(request_of).collect();
                    CopyDone {
                        request,
                        direction,
                        result: service.run(&requests),
                    }
                });
            });
        if let Err(error) = spawned {
            report(CopyDone {
                request,
                direction,
                result: CopyResult::Refused(CopyRefusal::Failed(error.to_string()).text()),
            });
        }
    }

    /// Run `work` with the engine's context: the compare rules, this service's provider and
    /// job factory, and a cancel token no one cancels.
    pub(super) fn with_context<R>(&self, work: impl FnOnce(&CopyContext<'_>) -> R) -> R {
        let cancel = CancelToken::new();
        let (options, fallback) = rules();
        work(&CopyContext {
            options: &options,
            fallback,
            ops: self.ops.as_ref(),
            factory: &self.factory,
            cancel: &cancel,
        })
    }

    /// Run the copy now and turn the engine's answer into the UI's.
    fn run(&self, requests: &[CopyRequest]) -> CopyResult {
        let ended = self.with_context(|ctx| match requests {
            [one] => copy_change(one, ctx).map(|_| ()),
            many => {
                let sheet = plan_sheet(many, ctx);
                run_sheet(&sheet, many, ctx).map(|_| ())
            }
        });
        match ended {
            Ok(()) => CopyResult::Copied,
            Err(refusal) => CopyResult::Refused(refusal.text()),
        }
    }
}

/// The refusal for an app that has no job factory (no profile folder).
pub(super) fn unavailable(change: &CopyChange, done: &CopySink) {
    done(CopyDone {
        request: change.pair.id,
        direction: change.direction,
        result: CopyResult::Refused(CopyRefusal::NotCopyable.text()),
    });
}
