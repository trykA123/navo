#[cfg(unix)]
mod close;
mod grid;
mod hooks;
mod prompt;
mod session;
mod shells;
#[cfg(windows)]
mod windows;

use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::Sink;

/// Generous: a loaded host can take seconds to start a shell.
const DEADLINE: Duration = Duration::from_secs(20);

/// What a `Recorder` has seen so far.
#[derive(Clone, Debug, Default)]
struct Recorded {
    output: Vec<u8>,
    exit: Option<Option<u32>>,
    exits: usize,
}

impl Recorded {
    fn printed(&self, text: &str) -> bool {
        self.output
            .windows(text.len())
            .any(|window| window == text.as_bytes())
    }
}

/// A sink that keeps everything and lets a test wait for a condition.
#[derive(Default)]
struct Recorder {
    seen: Mutex<Recorded>,
    changed: Condvar,
}

impl Recorder {
    fn update(&self, change: impl FnOnce(&mut Recorded)) {
        change(&mut self.seen.lock().unwrap_or_else(PoisonError::into_inner));
        self.changed.notify_all();
    }

    /// Waits until `done` holds; on timeout fails with the last state.
    fn wait_for(&self, done: impl Fn(&Recorded) -> bool) -> Recorded {
        self.wait_within(DEADLINE, done)
    }

    /// Like `wait_for` with a tighter `limit`.
    fn wait_within(&self, limit: Duration, done: impl Fn(&Recorded) -> bool) -> Recorded {
        let start = Instant::now();
        let mut seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        while !done(&seen) {
            let left = limit.checked_sub(start.elapsed());
            let Some(left) = left else {
                let output = String::from_utf8_lossy(&seen.output);
                panic!(
                    "condition not met in {limit:?}; exit {:?}, output {output:?}",
                    seen.exit
                );
            };
            seen = self
                .changed
                .wait_timeout(seen, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        seen.clone()
    }
}

impl Sink for Recorder {
    fn output(&self, bytes: &[u8]) {
        self.update(|seen| seen.output.extend_from_slice(bytes));
    }

    fn exited(&self, code: Option<u32>) {
        self.update(|seen| {
            seen.exit = Some(code);
            seen.exits += 1;
        });
    }
}
