//! One open file pair: a worker loads it, answers span requests and reloads on refresh.

use std::collections::VecDeque;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::{self, JoinHandle};

use navo_compare::model::{CancelToken, Reason};
use navo_ui::compare_diff::{DiffSink, DiffUpdate, PairControl, PairKind, PairRequest};

use super::convert::{spans, to_pair_text};
use super::load::{LoadInput, Loaded, load_pair};

const CAPACITY: usize = 8;

/// Builds the pair's two sides; it runs on the pair worker because it touches the file system.
pub(crate) type Opener = Box<dyn Fn() -> Result<Arc<LoadInput>, Reason> + Send>;

/// An open pair. `spans` and `refresh` only queue work; dropping cancels off the caller.
pub(crate) struct PairDiff {
    commands: Arc<Commands>,
    thread: Option<JoinHandle<()>>,
}

impl PairDiff {
    /// Start the worker for an input that is already built.
    pub(crate) fn start(request: PairRequest, input: LoadInput, sink: DiffSink) -> Self {
        let input = Arc::new(input);
        Self::open(request, Box::new(move || Ok(Arc::clone(&input))), sink)
    }

    /// Start the worker; `opener` builds the sides there, so nothing runs on the caller's thread.
    pub(crate) fn open(request: PairRequest, opener: Opener, sink: DiffSink) -> Self {
        let commands = Arc::new(Commands::new());
        let id = request.id;
        let failed = Arc::clone(&sink);
        let worker = Arc::clone(&commands);
        let spawned = thread::Builder::new()
            .name("compare-pair".to_owned())
            .spawn(move || run(id, &opener, sink, &worker));
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
        Self { commands, thread }
    }
}

impl PairControl for PairDiff {
    fn spans(&self, rows: Range<u32>) {
        self.commands.send(Command::Spans(rows));
    }

    fn refresh(&self) {
        self.commands.send(Command::Refresh);
    }
}

impl Drop for PairDiff {
    /// Cancel the load and join on a helper thread, never on the caller.
    fn drop(&mut self) {
        self.commands.close();
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

/// One command for the pair worker.
enum Command {
    Spans(Range<u32>),
    Refresh,
}

/// Work found after a load, before the next blocking receive.
#[derive(Default)]
struct Pending {
    spans: Option<Range<u32>>,
    refresh: bool,
}

/// The worker's bounded queue. A newer span request replaces an older one.
struct Commands {
    queue: Mutex<VecDeque<Command>>,
    ready: Condvar,
    closed: AtomicBool,
    cancel: CancelToken,
}

impl Commands {
    fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            ready: Condvar::new(),
            closed: AtomicBool::new(false),
            cancel: CancelToken::new(),
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
                Command::Refresh => pending.refresh = true,
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

/// Open the sides, load, report `Opened`, then answer spans and reopen on refresh until closed.
fn run(mut id: u64, opener: &Opener, sink: DiffSink, commands: &Commands) {
    loop {
        let Some(input) = open_input(&mut id, opener, &sink, commands) else {
            return;
        };
        let loaded = load_pair(&input, &commands.cancel);
        if commands.cancel.is_cancelled() {
            return;
        }
        let pending = commands.take_pending();
        if pending.refresh {
            id = id.wrapping_add(1);
            continue;
        }
        sink(DiffUpdate::Opened {
            request: id,
            kind: loaded.kind,
            text: to_pair_text(&loaded).map(Arc::new),
        });
        if let Some(rows) = pending.spans {
            send_spans(&loaded, id, rows, &sink);
        }
        loop {
            match commands.recv() {
                None => return,
                Some(Command::Spans(rows)) => send_spans(&loaded, id, rows, &sink),
                Some(Command::Refresh) => {
                    id = id.wrapping_add(1);
                    break;
                }
            }
        }
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
        wait_refresh(commands)?;
        *id = id.wrapping_add(1);
    }
}

/// Ignore span requests until a refresh (`Some`) or close (`None`).
fn wait_refresh(commands: &Commands) -> Option<()> {
    loop {
        match commands.recv()? {
            Command::Refresh => return Some(()),
            Command::Spans(_) => {}
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
