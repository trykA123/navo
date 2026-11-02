use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

use super::{
    ClipService, ClipSink, Clock, How, NoSealer, Ports, SealError, Sealer, SecretBytes, Sequence,
    SinkError, Snapshot,
};
use crate::limits::Limits;
use crate::model::{Captured, Content, Transfer};
use crate::store::ClipStore;

mod flow;
mod notes;
mod saved;
mod secrets;
mod stress;
mod timer;

const WAIT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Call {
    Text(String),
    Files(Vec<PathBuf>, bool),
    Private(String),
    Clear(Sequence, bool),
    ClearFailed(Sequence),
}

/// What the fake clipboard holds now and its change count.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Board {
    sequence: u32,
    text: Option<String>,
}

#[derive(Default)]
struct FakeSink {
    calls: Mutex<Vec<Call>>,
    board: Mutex<Board>,
    delay: Duration,
    fail: bool,
    /// Clears that fail before one works; `u32::MAX` fails every one.
    failing_clears: AtomicU32,
    entered: Option<Mutex<Sender<()>>>,
    release: Option<Mutex<Receiver<()>>>,
}

impl FakeSink {
    fn record(&self, call: Call) -> Result<(), SinkError> {
        if let Some(entered) = &self.entered {
            let _ = entered.lock().unwrap().send(());
        }
        if let Some(release) = &self.release {
            let _ = release.lock().unwrap().recv();
        }
        thread::sleep(self.delay);
        if self.fail {
            return Err(SinkError::Failed);
        }
        let mut board = self.board.lock().unwrap();
        board.sequence += 1;
        board.text = match &call {
            Call::Text(text) | Call::Private(text) => Some(text.clone()),
            _ => None,
        };
        self.calls.lock().unwrap().push(call);
        Ok(())
    }

    /// Another app copies `text`.
    fn copy_elsewhere(&self, text: &str) {
        let mut board = self.board.lock().unwrap();
        board.sequence += 1;
        board.text = Some(text.to_owned());
    }

    fn board(&self) -> Board {
        self.board.lock().unwrap().clone()
    }

    fn clears(&self) -> Vec<(Sequence, bool)> {
        let calls = self.calls();
        calls
            .into_iter()
            .filter_map(|call| match call {
                Call::Clear(sequence, cleared) => Some((sequence, cleared)),
                _ => None,
            })
            .collect()
    }

    fn failed_clears(&self) -> usize {
        let calls = self.calls();
        calls
            .iter()
            .filter(|call| matches!(call, Call::ClearFailed(_)))
            .count()
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }
}

impl ClipSink for FakeSink {
    fn write_text(&self, text: &str) -> Result<(), SinkError> {
        self.record(Call::Text(text.to_owned()))
    }

    fn write_files(&self, paths: &[PathBuf], cut: bool) -> Result<(), SinkError> {
        self.record(Call::Files(paths.to_vec(), cut))
    }

    fn write_private(&self, text: &str) -> Result<Sequence, SinkError> {
        self.record(Call::Private(text.to_owned()))?;
        Ok(Sequence(self.board().sequence))
    }

    fn clear_if_sequence(&self, sequence: Sequence) -> Result<bool, SinkError> {
        let failing = self.failing_clears.load(Ordering::SeqCst);
        if failing > 0 {
            if failing != u32::MAX {
                self.failing_clears.store(failing - 1, Ordering::SeqCst);
            }
            self.calls.lock().unwrap().push(Call::ClearFailed(sequence));
            return Err(SinkError::Failed);
        }
        let mut board = self.board.lock().unwrap();
        let cleared = board.sequence == sequence.0;
        if cleared {
            board.sequence += 1;
            board.text = None;
        }
        self.calls
            .lock()
            .unwrap()
            .push(Call::Clear(sequence, cleared));
        Ok(cleared)
    }
}

const SEAL_MARK: &[u8] = b"FAKESEAL:";

/// Seals by reversing and flipping bits behind a marker; remembers every input.
#[derive(Default)]
struct FakeSealer {
    inputs: Mutex<Vec<Vec<u8>>>,
}

fn fake_seal(plain: &[u8]) -> Vec<u8> {
    let mut sealed = SEAL_MARK.to_vec();
    sealed.extend(plain.iter().rev().map(|byte| byte ^ 0x5a));
    sealed
}

impl Sealer for FakeSealer {
    fn seal(&self, plain: &[u8]) -> Result<Vec<u8>, SealError> {
        self.inputs.lock().unwrap().push(plain.to_vec());
        Ok(fake_seal(plain))
    }

    fn unseal(&self, sealed: &[u8]) -> Result<SecretBytes, SealError> {
        let body = sealed.strip_prefix(SEAL_MARK).ok_or(SealError::Failed)?;
        let plain = body.iter().rev().map(|byte| byte ^ 0x5a).collect();
        Ok(SecretBytes::new(plain))
    }
}

/// A start time plus an offset the test moves forward; real time flows in only when
/// `flows` is set.
struct FakeClock {
    base: Instant,
    offset: Mutex<Duration>,
    flows: AtomicBool,
}

impl FakeClock {
    fn new() -> Self {
        Self {
            base: Instant::now(),
            offset: Mutex::new(Duration::ZERO),
            flows: AtomicBool::new(false),
        }
    }

    fn advance(&self, by: Duration) {
        *self.offset.lock().unwrap() += by;
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        let real = if self.flows.load(Ordering::SeqCst) {
            self.base.elapsed()
        } else {
            Duration::ZERO
        };
        self.base + *self.offset.lock().unwrap() + real
    }
}

struct Harness {
    dir: TempDir,
    sink: Arc<FakeSink>,
    sealer: Arc<FakeSealer>,
    clock: Arc<FakeClock>,
    service: ClipService,
}

impl Harness {
    fn db(&self) -> PathBuf {
        self.dir.path().join("clipboard.db")
    }

    /// Moves the fake clock and has the worker look at it.
    fn advance(&self, by: Duration) {
        self.clock.advance(by);
        assert!(self.service.wake());
    }
}

fn harness(sink: FakeSink, limits: Limits) -> Harness {
    harness_stopping(sink, limits, super::STOP_WAIT)
}

fn harness_stopping(sink: FakeSink, limits: Limits, stop_wait: Duration) -> Harness {
    start(sink, limits, stop_wait, None)
}

fn start(
    sink: FakeSink,
    limits: Limits,
    stop_wait: Duration,
    sealer: Option<Arc<dyn Sealer>>,
) -> Harness {
    let dir = TempDir::new().unwrap();
    let store = ClipStore::open(&dir.path().join("clipboard.db"), limits).unwrap();
    let sink = Arc::new(sink);
    let fake_sealer = Arc::new(FakeSealer::default());
    let clock = Arc::new(FakeClock::new());
    let ports = Ports {
        sink: sink.clone(),
        sealer: sealer.unwrap_or_else(|| fake_sealer.clone()),
        clock: clock.clone(),
    };
    let service = ClipService::start_with(store, limits, ports, stop_wait);
    Harness {
        dir,
        sink,
        sealer: fake_sealer,
        clock,
        service,
    }
}

fn without_sealer() -> Harness {
    start(
        FakeSink::default(),
        Limits::default(),
        super::STOP_WAIT,
        Some(Arc::new(NoSealer)),
    )
}

fn answer<T>(reply: super::Reply<T>) -> Result<T, super::FailKind> {
    reply.recv_timeout(WAIT).unwrap()
}

fn wait_until(ready: impl Fn() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !ready() {
        assert!(Instant::now() < deadline, "condition never held");
        thread::sleep(Duration::from_millis(5));
    }
}

fn blocking_sink() -> (FakeSink, Receiver<()>, Sender<()>) {
    let (entered_tx, entered_rx) = channel();
    let (release_tx, release_rx) = channel();
    let sink = FakeSink {
        entered: Some(Mutex::new(entered_tx)),
        release: Some(Mutex::new(release_rx)),
        ..FakeSink::default()
    };
    (sink, entered_rx, release_tx)
}

fn first_id(h: &Harness, body: &str) -> i64 {
    h.service.captured(text(body, 1));
    wait_for(&h.service, |s| has_text(s, body)).history[0].id
}

fn plain() -> Harness {
    harness(FakeSink::default(), Limits::default())
}

fn text(body: &str, at_ms: i64) -> Captured {
    Captured {
        content: Content::Text(body.to_owned()),
        source_app: None,
        at_ms,
    }
}

fn files(paths: &[&str], transfer: Transfer, at_ms: i64) -> Captured {
    Captured {
        content: Content::Paths {
            paths: paths.iter().map(PathBuf::from).collect(),
            transfer,
        },
        source_app: None,
        at_ms,
    }
}

fn wait_for(service: &ClipService, ready: impl Fn(&Snapshot) -> bool) -> Arc<Snapshot> {
    let deadline = Instant::now() + WAIT;
    loop {
        let snapshot = service.snapshot();
        if ready(&snapshot) {
            return snapshot;
        }
        assert!(Instant::now() < deadline, "snapshot never matched");
        thread::sleep(Duration::from_millis(5));
    }
}

fn has_text(snapshot: &Snapshot, body: &str) -> bool {
    snapshot
        .history
        .iter()
        .any(|row| row.entry.content == Content::Text(body.to_owned()))
}

fn bodies(snapshot: &Snapshot) -> Vec<String> {
    snapshot
        .history
        .iter()
        .map(|row| match &row.entry.content {
            Content::Text(body) => body.clone(),
            Content::Paths { .. } => "<paths>".to_owned(),
        })
        .collect()
}
