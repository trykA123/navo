//! One open file pair: a worker loads it, answers span requests and reloads on refresh.

use std::collections::VecDeque;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::{self, JoinHandle};

use navo_compare::model::{CancelToken, Reason};
use navo_ui::compare_diff::{DiffSink, DiffUpdate, HexAlign, PairControl, PairKind, PairRequest};

use super::bytes::ByteDiff;
use super::convert::{spans, to_pair_text};
use super::image;
use super::load::{LoadInput, Loaded, load_pair};
use engine::{EngineJob, Live};
use hex_bytes::{clear_bytes, force_bytes, start_bytes};

mod engine;
mod hex_bytes;

pub(crate) use engine::{EnginePlan, large_input, spool_path};

const CAPACITY: usize = 8;

/// Builds the pair's two sides; it runs on the pair worker because it touches the file system.
pub(crate) type Opener = Box<dyn Fn() -> Result<Arc<LoadInput>, Reason> + Send>;

/// An open pair. `spans` and `refresh` only queue work; dropping cancels off the caller.
pub(crate) struct PairDiff {
    commands: Arc<Commands>,
    live: Arc<Live>,
    thread: Option<JoinHandle<()>>,
    #[cfg(test)]
    done: std::sync::mpsc::Receiver<()>,
}

impl PairDiff {
    /// Start the worker for an input that is already built, with default engine settings.
    pub(crate) fn start(request: PairRequest, input: LoadInput, sink: DiffSink) -> Self {
        let input = Arc::new(input);
        Self::open(
            request,
            Box::new(move || Ok(Arc::clone(&input))),
            EnginePlan::default(),
            sink,
        )
    }

    /// Start the worker; `opener` builds the sides there, so nothing runs on the caller's thread.
    pub(crate) fn open(
        request: PairRequest,
        opener: Opener,
        plan: EnginePlan,
        sink: DiffSink,
    ) -> Self {
        let commands = Arc::new(Commands::new());
        let live = Arc::new(Live::new());
        let id = request.id;
        let failed = Arc::clone(&sink);
        let worker = Arc::clone(&commands);
        let worker_live = Arc::clone(&live);
        #[cfg(test)]
        let (done_tx, done) = std::sync::mpsc::channel();
        let spawned = thread::Builder::new()
            .name("compare-pair".to_owned())
            .spawn(move || {
                run(&Worker {
                    request: &request,
                    opener: &opener,
                    plan: &plan,
                    sink: &sink,
                    commands: &worker,
                    live: &worker_live,
                });
                #[cfg(test)]
                let _ = done_tx.send(());
            });
        let thread = match spawned {
            Ok(thread) => Some(thread),
            Err(error) => {
                tracing::warn!(%error, "compare pair thread did not start");
                failed(DiffUpdate::Failed {
                    request: id,
                    text: navo_l10n::tr("compare-could-not-start"),
                });
                None
            }
        };
        Self {
            commands,
            live,
            thread,
            #[cfg(test)]
            done,
        }
    }
}

impl PairControl for PairDiff {
    fn spans(&self, rows: Range<u32>) {
        self.commands.send(Command::Spans(rows));
    }

    fn refresh(&self, request: u64) {
        self.commands.send(Command::Refresh(request));
    }

    fn view_as(&self, request: u64, bytes: bool) {
        self.commands.bytes_view.store(bytes, Ordering::Release);
        self.commands.send(Command::Refresh(request));
    }

    fn focus(&self, line: u64) {
        self.live.focus(line);
    }

    fn lines(&self, rows: Range<u64>) {
        self.live.lines(rows);
    }

    fn acknowledge(&self, upto: u64) {
        self.live.acknowledge(upto);
    }

    fn byte_regions(&self, align: HexAlign, run: u64) {
        self.commands.send(Command::Bytes { align, run });
    }
}

impl Drop for PairDiff {
    /// Cancel the load and the engine, then join on a helper thread, never on the caller.
    fn drop(&mut self) {
        self.commands.close();
        self.live.close();
        let Some(thread) = self.thread.take() else {
            return;
        };
        let joined = thread::Builder::new()
            .name("compare-pair-join".to_owned())
            .spawn(move || {
                if thread.join().is_err() {
                    tracing::warn!("compare pair thread panicked");
                }
            });
        if let Err(error) = joined {
            tracing::warn!(%error, "compare pair join thread did not start");
        }
    }
}

#[cfg(test)]
impl PairDiff {
    /// Cancel the worker before a test releases its blocked read.
    pub(crate) fn close_for_test(&self) {
        self.commands.close();
    }

    /// Wait up to 5 s for the worker to signal that it exited.
    pub(crate) fn wait_for_exit(&self) -> bool {
        self.done
            .recv_timeout(std::time::Duration::from_secs(5))
            .is_ok()
    }
}

/// One command for the pair worker.
enum Command {
    Spans(Range<u32>),
    Refresh(u64),
    Bytes { align: HexAlign, run: u64 },
}

/// Work found after a load, before the next blocking receive.
#[derive(Default)]
struct Pending {
    spans: Option<Range<u32>>,
    /// The id of the newest refresh, which coalesces every earlier one.
    refresh: Option<u64>,
    /// The newest byte alignment asked for.
    bytes: Option<(HexAlign, u64)>,
}

/// The worker's bounded queue. A newer span request replaces an older one.
struct Commands {
    queue: Mutex<VecDeque<Command>>,
    ready: Condvar,
    closed: AtomicBool,
    cancel: CancelToken,
    /// The running byte diff; dropping it cancels its workers.
    bytes: Mutex<Option<ByteDiff>>,
    /// Whether the pair opens as bytes whatever its kind (Alt+V), kept across refreshes.
    bytes_view: AtomicBool,
}

impl Commands {
    fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            ready: Condvar::new(),
            closed: AtomicBool::new(false),
            cancel: CancelToken::new(),
            bytes: Mutex::new(None),
            bytes_view: AtomicBool::new(false),
        }
    }

    /// Queue a command; drop the oldest when full and keep only the newest spans request.
    fn send(&self, command: Command) {
        let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
        if matches!(command, Command::Spans(_)) {
            queue.retain(|queued| !matches!(queued, Command::Spans(_)));
        }
        while queue.len() >= CAPACITY {
            queue.pop_front();
        }
        queue.push_back(command);
        drop(queue);
        self.ready.notify_one();
    }

    /// Take everything pending without blocking; the newest spans request wins.
    fn take_pending(&self) -> Pending {
        let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
        let mut pending = Pending::default();
        while let Some(command) = queue.pop_front() {
            match command {
                Command::Spans(rows) => pending.spans = Some(rows),
                Command::Refresh(next) => pending.refresh = Some(next),
                Command::Bytes { align, run } => pending.bytes = Some((align, run)),
            }
        }
        pending
    }

    /// Wait for the next command; `None` after `close`.
    fn recv(&self) -> Option<Command> {
        let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(command) = queue.pop_front() {
                return Some(command);
            }
            if self.closed.load(Ordering::Acquire) {
                return None;
            }
            queue = self
                .ready
                .wait(queue)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Stop the worker and wake it if it waits.
    fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.cancel.cancel();
        self.ready.notify_all();
    }
}

/// Everything one pair worker thread owns; a struct keeps the worker call readable.
struct Worker<'a> {
    request: &'a PairRequest,
    opener: &'a Opener,
    plan: &'a EnginePlan,
    sink: &'a DiffSink,
    commands: &'a Commands,
    live: &'a Live,
}

/// Open the sides, load, report `Opened`, then answer spans and restart on refresh until closed.
fn run(worker: &Worker<'_>) {
    let _guard = LiveGuard(worker.live);
    let mut id = worker.request.id;
    loop {
        let Some(input) = open_input(&mut id, worker.opener, worker.sink, worker.commands) else {
            return;
        };
        let loaded = load_forced(&input, worker.commands);
        if worker.commands.cancel.is_cancelled() {
            return;
        }
        let pending = worker.commands.take_pending();
        if let Some(next) = pending.refresh {
            id = next;
            worker.live.clear();
            clear_bytes(worker.commands);
            continue;
        }
        (worker.sink)(DiffUpdate::Opened {
            request: id,
            kind: loaded.kind,
            text: to_pair_text(&loaded).map(Arc::new),
        });
        if let Some(rows) = pending.spans {
            send_spans(&loaded, id, rows, worker.sink);
        }
        let mut pair = worker.request.clone();
        pair.id = id;
        worker.live.install(
            EngineJob {
                request: &pair,
                loaded: &loaded,
                load: &input,
                plan: worker.plan,
                sink: worker.sink,
            }
            .running(),
        );
        if matches!(loaded.kind, PairKind::Image)
            && !image::diff(&input, id, worker.sink, &worker.commands.cancel)
        {
            return;
        }
        if let Some((align, run)) = pending.bytes {
            start_bytes((worker.request, &input, id), (align, run), worker);
        }
        let Some(next) = serve((&loaded, &input), id, worker) else {
            return;
        };
        id = next;
    }
}

/// Load the pair; as bytes when Alt+V forced that view, which every reload keeps.
fn load_forced(input: &LoadInput, commands: &Commands) -> Loaded {
    let mut loaded = load_pair(input, &commands.cancel);
    force_bytes(&mut loaded, commands.bytes_view.load(Ordering::Acquire));
    loaded
}

/// Answer spans and byte requests until a refresh gives the next request id (`Some`) or the pair closes (`None`).
fn serve((loaded, input): (&Loaded, &LoadInput), id: u64, worker: &Worker<'_>) -> Option<u64> {
    loop {
        match worker.commands.recv()? {
            Command::Spans(rows) => send_spans(loaded, id, rows, worker.sink),
            Command::Refresh(next) => {
                worker.live.clear();
                clear_bytes(worker.commands);
                return Some(next);
            }
            Command::Bytes { align, run } => {
                start_bytes((worker.request, input, id), (align, run), worker);
            }
        }
    }
}

/// Clears the live engine when the pair worker stops, whatever path it takes.
struct LiveGuard<'a>(&'a Live);

impl Drop for LiveGuard<'_> {
    fn drop(&mut self) {
        self.0.close();
    }
}

/// Build the sides; while they cannot open, report Unavailable and wait for a refresh.
/// `None` once the pair is closed.
fn open_input(
    id: &mut u64,
    opener: &Opener,
    sink: &DiffSink,
    commands: &Commands,
) -> Option<Arc<LoadInput>> {
    loop {
        let reason = match opener() {
            Ok(input) => return Some(input),
            Err(reason) => reason,
        };
        if commands.cancel.is_cancelled() {
            return None;
        }
        sink(DiffUpdate::Opened {
            request: *id,
            kind: PairKind::Unavailable(reason.fluent_id().as_str()),
            text: None,
        });
        *id = wait_refresh(commands)?;
    }
}

/// Ignore span requests until a refresh (`Some` with its id) or close (`None`).
fn wait_refresh(commands: &Commands) -> Option<u64> {
    loop {
        match commands.recv()? {
            Command::Refresh(next) => return Some(next),
            Command::Spans(_) | Command::Bytes { .. } => {}
        }
    }
}

/// Convert and send the spans of one visible row range.
fn send_spans(loaded: &Loaded, request: u64, rows: Range<u32>, sink: &DiffSink) {
    sink(DiffUpdate::Spans {
        request,
        spans: spans(loaded, rows),
    });
}
