//! The output path. The reader never waits for the sink: it appends to a bounded buffer that
//! coalesces chunks and drops (with a warning) past its limit. A sink thread hands the buffer
//! on, then the exit code, and stops waiting for the reader 2 s after the shell was reaped (R2).

use std::io::{self, Read};
use std::ops::ControlFlow;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use super::run_thread;
use crate::Sink;

const READ_CHUNK: usize = 16 * 1024;
/// Output held for a sink that falls behind; beyond this the reader drops bytes.
const OUTBOX_LIMIT: usize = 4 * 1024 * 1024;
/// R2: how long after the shell was reaped the sink thread still waits for the reader.
const READER_LIMIT: Duration = Duration::from_secs(2);

/// Bytes read but not yet delivered, plus the shell's exit; shared by the reader, the
/// child's owner and the sink thread.
#[derive(Default)]
pub(super) struct Outbox {
    state: Mutex<Pending>,
    ready: Condvar,
}

#[derive(Default)]
struct Pending {
    bytes: Vec<u8>,
    dropped: usize,
    /// The reader reached EOF or a read error.
    ended: bool,
    /// The shell was reaped: its exit code (if readable) and when.
    exit: Option<(Option<u32>, Instant)>,
    /// The session never started or the sink thread finished: discard everything.
    closed: bool,
}

impl Pending {
    fn append(&mut self, bytes: &[u8]) {
        if self.closed {
            return;
        }
        let room = OUTBOX_LIMIT.saturating_sub(self.bytes.len());
        let (kept, lost) = bytes.split_at(room.min(bytes.len()));
        self.bytes.extend_from_slice(kept);
        if lost.is_empty() {
            return;
        }
        if self.dropped == 0 {
            tracing::warn!("terminal output buffer full; dropping output");
        }
        self.dropped = self.dropped.saturating_add(lost.len());
    }

    /// The exit code once the reader ended or ran out of time, else how long to wait.
    fn exit_or_wait(&self) -> ControlFlow<Option<u32>, Duration> {
        let Some((code, at)) = self.exit else {
            return ControlFlow::Continue(READER_LIMIT);
        };
        if self.ended {
            return ControlFlow::Break(code);
        }
        match READER_LIMIT.checked_sub(at.elapsed()) {
            Some(left) if !left.is_zero() => ControlFlow::Continue(left),
            _ => {
                tracing::warn!("terminal reader still running 2 s after the shell ended; detached");
                ControlFlow::Break(code)
            }
        }
    }
}

/// What the sink thread does next.
enum Next {
    Deliver(Vec<u8>),
    Exit(Option<u32>),
    Quit,
}

impl Outbox {
    fn lock(&self) -> MutexGuard<'_, Pending> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn update(&self, change: impl FnOnce(&mut Pending)) {
        change(&mut self.lock());
        self.ready.notify_all();
    }

    /// Appends `bytes` without waiting; whatever exceeds the limit is dropped and counted.
    pub(super) fn push(&self, bytes: &[u8]) {
        self.update(|state| state.append(bytes));
    }

    /// Marks the end of output; the sink thread finishes once it took the rest.
    fn end(&self) {
        self.update(|state| state.ended = true);
    }

    /// The child's owner reaped the shell; starts the reader's 2 s limit.
    pub(super) fn exited(&self, code: Option<u32>) {
        self.update(|state| state.exit = Some((code, Instant::now())));
    }

    /// The session failed to start or the sink heard the exit: later output is discarded.
    pub(super) fn close(&self) {
        self.update(|state| {
            state.closed = true;
            state.bytes = Vec::new();
        });
    }

    /// Waits for output or the end; output always goes first, the exit code last.
    fn next(&self) -> Next {
        let mut state = self.lock();
        loop {
            if state.closed {
                return Next::Quit;
            }
            if state.dropped > 0 {
                tracing::warn!(
                    bytes = state.dropped,
                    "terminal output dropped; the view fell behind"
                );
                state.dropped = 0;
            }
            if !state.bytes.is_empty() {
                return Next::Deliver(std::mem::take(&mut state.bytes));
            }
            let wait = match state.exit_or_wait() {
                ControlFlow::Continue(wait) => wait,
                ControlFlow::Break(code) => return Next::Exit(code),
            };
            state = self
                .ready
                .wait_timeout(state, wait)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// Whether the reader ended within `limit`.
    #[cfg(all(test, unix))]
    pub(super) fn ended_within(&self, limit: Duration) -> bool {
        let state = self.lock();
        let waited = self
            .ready
            .wait_timeout_while(state, limit, |state| !state.ended);
        !waited.unwrap_or_else(PoisonError::into_inner).1.timed_out()
    }
}

/// Starts the reader: it drains until EOF or a read error (a closed PTY reports EIO on Unix)
/// and drops the read handle as it ends, so a closing ConPTY never waits on it. On failure
/// the read handle is dropped before this returns.
pub(super) fn read(output: Box<dyn Read + Send>, outbox: &Arc<Outbox>) -> io::Result<()> {
    let outbox = Arc::clone(outbox);
    let started = run_thread("navo-term-read", output, move |mut output| {
        drain(&mut *output, &outbox);
        drop(output);
        outbox.end();
    });
    started.map(drop).map_err(|(error, _output)| error)
}

fn drain(output: &mut dyn Read, outbox: &Outbox) {
    let mut buffer = vec![0; READ_CHUNK];
    loop {
        match output.read(&mut buffer) {
            Ok(0) => return,
            Ok(read) => outbox.push(buffer.get(..read).unwrap_or_default()),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => {
                tracing::debug!(%error, "terminal output closed");
                return;
            }
        }
    }
}

/// Starts the sink thread: it delivers output in order, then the exit code, once.
pub(super) fn deliver(outbox: &Arc<Outbox>, sink: Arc<dyn Sink>) -> io::Result<()> {
    let outbox = Arc::clone(outbox);
    let started = run_thread("navo-term-sink", sink, move |sink| {
        loop {
            match outbox.next() {
                Next::Deliver(bytes) => sink.output(&bytes),
                Next::Exit(code) => {
                    sink.exited(code);
                    break;
                }
                Next::Quit => break,
            }
        }
        outbox.close();
    });
    started.map(drop).map_err(|(error, _sink)| error)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{Next, OUTBOX_LIMIT, Outbox};

    fn delivered(next: Next) -> Option<usize> {
        match next {
            Next::Deliver(bytes) => Some(bytes.len()),
            Next::Exit(_) | Next::Quit => None,
        }
    }

    #[test]
    fn the_outbox_coalesces_chunks_and_drops_past_its_limit_without_waiting() {
        let outbox = Outbox::default();
        outbox.push(b"ab");
        outbox.push(b"cd");
        assert_eq!(delivered(outbox.next()), Some(4));
        let chunk = vec![b'x'; OUTBOX_LIMIT / 2 + 1];
        for _ in 0..3 {
            outbox.push(&chunk);
        }
        outbox.end();
        outbox.exited(Some(0));
        assert_eq!(delivered(outbox.next()), Some(OUTBOX_LIMIT));
        assert!(matches!(outbox.next(), Next::Exit(Some(0))));
    }

    #[test]
    fn the_exit_waits_for_a_stuck_reader_at_most_the_limit() {
        let outbox = Outbox::default();
        outbox.exited(Some(3));
        let start = Instant::now();
        assert!(matches!(outbox.next(), Next::Exit(Some(3))));
        let waited = start.elapsed();
        assert!(waited >= Duration::from_millis(1_900), "{waited:?}");
        assert!(waited < Duration::from_secs(5), "{waited:?}");
    }

    #[test]
    fn a_closed_outbox_quits_and_discards_output() {
        let outbox = Outbox::default();
        outbox.close();
        outbox.push(b"late");
        assert!(matches!(outbox.next(), Next::Quit));
    }
}
