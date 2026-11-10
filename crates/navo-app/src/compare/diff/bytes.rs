//! Byte diffs on workers (V36, D18): region deltas and progress for the hex view.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use navo_compare::bytes::{Align, ByteDiff as Engine, ByteEvent, ByteRegion, ByteSide, Request};
use navo_compare::model::{Budget, CancelToken, DiffStep, Reason};
use navo_ui::compare_diff::{ByteProgress, ByteRegionView, DiffSink, DiffUpdate, PairRequest};

use super::load::Side;

const POLL: Duration = Duration::from_millis(20);

/// Both sides, the alignment and the memory budget for one byte diff.
pub(crate) struct ByteInput {
    pub left: Side,
    pub right: Side,
    pub align: Align,
    pub budget: Budget,
}

/// An open byte diff; dropping it cancels and reaps both workers off the caller.
pub(crate) struct ByteDiff {
    commands: Arc<Commands>,
    thread: Option<JoinHandle<Engine>>,
    #[cfg(test)]
    joined: Arc<AtomicBool>,
}

impl ByteDiff {
    /// Start the adapter; it owns the engine and returns it for reaping.
    pub(crate) fn start(request: PairRequest, input: ByteInput, sink: DiffSink) -> Self {
        let commands = Arc::new(Commands::new());
        let id = request.id;
        let worker = Arc::clone(&commands);
        let total = match input.align {
            Align::Offset => input.left.1.size.max(input.right.1.size),
            Align::Content => input.left.1.size.saturating_add(input.right.1.size),
        };
        #[cfg(test)]
        let joined = Arc::default();
        let spawned = thread::Builder::new()
            .name("compare-bytes".to_owned())
            .spawn(move || {
                let engine = Engine::start(request_of(input), &worker.cancel);
                let mut report = Report::new(id, total, sink);
                run(&engine, &worker, &mut report);
                engine
            });
        let thread = match spawned {
            Ok(thread) => Some(thread),
            Err(error) => {
                tracing::warn!(%error, "compare byte thread did not start");
                None
            }
        };
        Self {
            commands,
            thread,
            #[cfg(test)]
            joined,
        }
    }

    /// The flag the reaper sets after both workers joined, for tests that drop the control.
    #[cfg(test)]
    pub(crate) fn join_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.joined)
    }
}

impl Drop for ByteDiff {
    /// Cancel, then join the adapter and the engine on one reaper thread.
    fn drop(&mut self) {
        self.commands.close();
        let Some(thread) = self.thread.take() else {
            return;
        };
        #[cfg(test)]
        let joined = Arc::clone(&self.joined);
        let reaped = thread::Builder::new()
            .name("compare-bytes-join".to_owned())
            .spawn(move || {
                match thread.join() {
                    Ok(engine) => engine.join(),
                    Err(_) => tracing::warn!("compare byte thread panicked"),
                }
                #[cfg(test)]
                joined.store(true, Ordering::Release);
            });
        if let Err(error) = reaped {
            tracing::warn!(%error, "compare byte join thread did not start");
        }
    }
}

/// The worker's cancel flag and token.
struct Commands {
    closed: AtomicBool,
    cancel: CancelToken,
}

impl Commands {
    /// A queue-free control that only cancels.
    fn new() -> Self {
        Self {
            closed: AtomicBool::new(false),
            cancel: CancelToken::new(),
        }
    }

    /// Stop the worker and wake the engine.
    fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.cancel.cancel();
    }
}

/// Turn the adapter input into an engine request; sizes are not trusted here.
fn request_of(input: ByteInput) -> Request {
    Request {
        left: ByteSide::new(input.left.0, input.left.1),
        right: ByteSide::new(input.right.0, input.right.1),
        align: input.align,
        budget: input.budget,
    }
}

/// Forward engine events until `Done`, a failure or a cancel.
fn run(engine: &Engine, commands: &Commands, report: &mut Report) {
    loop {
        if commands.closed.load(Ordering::Acquire) || commands.cancel.is_cancelled() {
            break;
        }
        match engine.events().recv_timeout(POLL) {
            Ok(event) => {
                if report.apply(event) {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// The new regions and progress since the last snapshot.
struct Report {
    id: u64,
    regions: Vec<ByteRegionView>,
    progress: ByteProgress,
    sink: DiffSink,
}

impl Report {
    /// A report with no regions and `total` expected bytes.
    fn new(id: u64, total: u64, sink: DiffSink) -> Self {
        Self {
            id,
            regions: Vec::new(),
            progress: ByteProgress {
                done: 0,
                total,
                coarse: false,
            },
            sink,
        }
    }

    /// Apply one engine event; true when the diff is over.
    fn apply(&mut self, event: ByteEvent) -> bool {
        match event {
            ByteEvent::Region(region) => self.regions.push(region_view(&region)),
            ByteEvent::Progress { done, total } => {
                self.progress.done = done.min(total.saturating_sub(1));
                self.progress.total = total;
            }
            ByteEvent::Step(DiffStep::Coarse) => self.progress.coarse = true,
            ByteEvent::Step(_) => return false,
            ByteEvent::Done => {
                self.progress.done = self.progress.total;
                self.send();
                return true;
            }
            ByteEvent::Failed(reason) => {
                if reason != Reason::Stopped {
                    (self.sink)(DiffUpdate::Failed {
                        request: self.id,
                        text: failure_text(reason),
                    });
                }
                return true;
            }
        }
        self.send();
        false
    }

    /// Send only the regions added since the previous snapshot.
    fn send(&mut self) {
        let regions = std::mem::take(&mut self.regions);
        (self.sink)(DiffUpdate::Bytes {
            request: self.id,
            regions,
            progress: Some(self.progress),
        });
    }
}

/// Convert an engine region to the UI's offsets.
fn region_view(region: &ByteRegion) -> ByteRegionView {
    ByteRegionView {
        left: region.left.clone(),
        right: region.right.clone(),
        changed: region.changed,
    }
}

/// The translated reason for a failed byte diff.
fn failure_text(reason: Reason) -> String {
    navo_l10n::tr(reason.fluent_id().as_str())
}
