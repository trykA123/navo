//! The large diff's command queues: focus and acknowledgements for the drain worker, and
//! line windows for the display reader.

use std::collections::VecDeque;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, PoisonError};

use navo_compare::model::CancelToken;

const CAPACITY: usize = 8;
const LINE_CAPACITY: usize = 4;

/// One command for the drain worker.
pub(crate) enum Command {
    Focus(u64),
    Ack(u64),
}

/// The drain worker's bounded queue and the acknowledgement counter tests read.
pub(crate) struct Commands {
    queue: Mutex<VecDeque<Command>>,
    pub(crate) closed: AtomicBool,
    pub(crate) cancel: CancelToken,
}

impl Commands {
    /// A queue whose cancel token the engine also holds.
    pub(crate) fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            closed: AtomicBool::new(false),
            cancel: CancelToken::new(),
        }
    }

    /// Queue focus, replacing any waiting focus request.
    pub(crate) fn focus(&self, line: u64) {
        let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
        queue.retain(|command| !matches!(command, Command::Focus(_)));
        while queue.len() >= CAPACITY {
            queue.pop_front();
        }
        queue.push_back(Command::Focus(line));
    }

    /// Queue an acknowledgement, keeping only the highest waiting one.
    pub(crate) fn acknowledge(&self, upto: u64) {
        let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
        let highest = queue.iter().filter_map(|command| match command {
            Command::Ack(seq) => Some(*seq),
            Command::Focus(_) => None,
        });
        let highest = highest.fold(upto, u64::max);
        queue.retain(|command| !matches!(command, Command::Ack(_)));
        queue.push_back(Command::Ack(highest));
    }

    /// Take the next command without blocking.
    pub(crate) fn next(&self) -> Option<Command> {
        self.queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
    }

    /// Stop both workers and wake the engine.
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.cancel.cancel();
    }
}

/// The waiting windows and the closed flag, guarded by one lock so a close is never missed.
#[derive(Default)]
pub(crate) struct Lines {
    pub(crate) rows: VecDeque<Range<u64>>,
    pub(crate) closed: bool,
}

/// The line worker's bounded queue; a newer request replaces the older pending one.
pub(crate) struct LineQueue {
    pub(crate) lines: Mutex<Lines>,
    pub(crate) ready: Condvar,
    pub(crate) cancel: CancelToken,
}

impl LineQueue {
    /// A queue with its own cancel token, cancelled only when the view closes it.
    pub(crate) fn new(cancel: CancelToken) -> Self {
        Self {
            lines: Mutex::new(Lines::default()),
            ready: Condvar::new(),
            cancel,
        }
    }

    /// Add the newest window and wake the line worker.
    pub(crate) fn push(&self, rows: Range<u64>) {
        let mut lines = self.lines.lock().unwrap_or_else(PoisonError::into_inner);
        lines.rows.clear();
        while lines.rows.len() >= LINE_CAPACITY {
            lines.rows.pop_front();
        }
        lines.rows.push_back(rows);
        drop(lines);
        self.ready.notify_one();
    }

    /// Wait for the next window; `None` once closed.
    pub(crate) fn next(&self) -> Option<Range<u64>> {
        let mut lines = self.lines.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(rows) = lines.rows.pop_front() {
                return Some(rows);
            }
            if lines.closed {
                return None;
            }
            lines = self
                .ready
                .wait(lines)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Whether the view closed this queue.
    pub(crate) fn is_closed(&self) -> bool {
        self.lines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .closed
    }

    /// Stop the line worker: set `closed` under the lock, then wake it, so no wake-up is lost.
    pub(crate) fn close(&self) {
        self.lines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .closed = true;
        self.cancel.cancel();
        self.ready.notify_all();
    }
}
