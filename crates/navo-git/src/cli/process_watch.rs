use std::collections::VecDeque;
use std::io::{self, Read};
use std::process::Output;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::GitError;

use super::process_tree::GitTree;

pub(super) type Outcome = Arc<Mutex<Option<Result<Output, GitError>>>>;
pub(super) type ChildSlot = Arc<Mutex<Option<GitTree>>>;

/// A permanent cancellation flag shared by every clone.
#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessOutcome {
    Done,
    Cancelled,
    TimedOut,
    Capped,
}

pub(super) struct Control {
    pub cancel: Cancel,
    pub capped: Arc<AtomicBool>,
}

impl Control {
    fn reason(&self) -> Option<ProcessOutcome> {
        if self.cancel.is_cancelled() {
            Some(ProcessOutcome::Cancelled)
        } else if self.capped.load(Ordering::Acquire) {
            Some(ProcessOutcome::Capped)
        } else {
            None
        }
    }
}

/// How often an active watchdog with a file-operation stop checks its flags.
const STOP_POLL: Duration = Duration::from_millis(5);

/// How often an idle filter stream without a deadline checks its stop flag.
const IDLE_STOP_POLL: Duration = Duration::from_millis(20);

pub(super) struct Watchdog {
    activity: Arc<Activity>,
    handle: Option<JoinHandle<Result<(), GitError>>>,
    outcome: Outcome,
}

struct Activity {
    state: Mutex<Idle>,
    changed: Condvar,
    control: Option<Control>,
    stop: Option<Cancel>,
}

#[derive(Default)]
struct Idle {
    since: Option<Instant>,
    stopped: bool,
    reason: Option<ProcessOutcome>,
}

impl Watchdog {
    #[expect(
        clippy::too_many_arguments,
        reason = "the start carries the child, timeout, outcome, cancel control and stop flag"
    )]
    pub(super) fn start(
        child: ChildSlot,
        timeout: Duration,
        outcome: Outcome,
        control: Option<Control>,
        stop: Option<Cancel>,
    ) -> Result<Self, GitError> {
        let activity = Arc::new(Activity {
            state: Mutex::new(Idle {
                since: control.as_ref().map(|_| Instant::now()),
                ..Idle::default()
            }),
            changed: Condvar::new(),
            control,
            stop,
        });
        let watched = Arc::clone(&activity);
        let handle = thread::Builder::new()
            .name("git-filter-watchdog".to_owned())
            .spawn(move || watch(watched, child, timeout))
            .map_err(git_io)?;
        Ok(Self {
            activity,
            handle: Some(handle),
            outcome,
        })
    }

    pub(super) fn set_active(&self, active: bool) {
        self.activity
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .since = active.then(Instant::now);
        self.activity.changed.notify_one();
    }

    pub(super) fn timed_out(&self) -> bool {
        self.reason() == Some(ProcessOutcome::TimedOut)
    }
    pub(super) fn finish(mut self) -> Option<ProcessOutcome> {
        self.stop();
        self.reason()
    }

    fn reason(&self) -> Option<ProcessOutcome> {
        self.activity
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .reason
    }

    fn stop(&mut self) {
        self.activity
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .stopped = true;
        self.activity.changed.notify_one();
        if let Some(handle) = self.handle.take() {
            let result = handle
                .join()
                .map_err(|_| GitError::SessionClosed)
                .and_then(|result| result);
            if let Err(error) = result {
                *self.outcome.lock().unwrap_or_else(PoisonError::into_inner) = Some(Err(error));
            }
        }
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.stop();
    }
}

fn watch(activity: Arc<Activity>, child: ChildSlot, timeout: Duration) -> Result<(), GitError> {
    let mut state = activity
        .state
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    loop {
        if state.stopped {
            return Ok(());
        }
        if activity.stop.as_ref().is_some_and(Cancel::is_cancelled) {
            state.reason = Some(ProcessOutcome::Cancelled);
            drop(state);
            return stop_shared(&child);
        }
        let Some(since) = state.since else {
            state = if activity.stop.is_some() {
                activity
                    .changed
                    .wait_timeout(state, IDLE_STOP_POLL)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0
            } else {
                activity
                    .changed
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner)
            };
            continue;
        };
        let remaining = timeout.saturating_sub(since.elapsed());
        let reason = activity
            .control
            .as_ref()
            .and_then(Control::reason)
            .or_else(|| remaining.is_zero().then_some(ProcessOutcome::TimedOut));
        if let Some(reason) = reason {
            state.reason = Some(reason);
            drop(state);
            return stop_shared(&child);
        }
        let remaining = if activity.control.is_some() || activity.stop.is_some() {
            remaining.min(STOP_POLL)
        } else {
            remaining
        };
        state = activity
            .changed
            .wait_timeout(state, remaining)
            .unwrap_or_else(PoisonError::into_inner)
            .0;
    }
}

fn git_io(error: io::Error) -> GitError {
    GitError::Io {
        path: "git filter".into(),
        kind: error.kind(),
    }
}

pub(super) fn stop_shared(child: &ChildSlot) -> Result<(), GitError> {
    let owned = child.lock().unwrap_or_else(PoisonError::into_inner).take();
    let Some(mut owned) = owned else {
        return Ok(());
    };
    let result = owned.stop();
    *child.lock().unwrap_or_else(PoisonError::into_inner) = Some(owned);
    result
}

const STDERR_CAPACITY: usize = 8 * 1024;

/// The first and the last `STDERR_CAPACITY` bytes, joined by `\n...\n` when bytes were dropped.
pub(super) fn capture_stderr(mut stderr: impl Read) -> io::Result<Vec<u8>> {
    let mut kept = HeadTail::default();
    let mut chunk = [0; 4096];
    loop {
        match stderr.read(&mut chunk) {
            Ok(0) => return Ok(kept.joined()),
            Ok(read) => kept.push(&chunk[..read]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

#[derive(Default)]
struct HeadTail {
    head: Vec<u8>,
    tail: VecDeque<u8>,
    dropped: bool,
}

impl HeadTail {
    fn push(&mut self, bytes: &[u8]) {
        let room = STDERR_CAPACITY
            .saturating_sub(self.head.len())
            .min(bytes.len());
        let (first, rest) = bytes.split_at(room);
        self.head.extend_from_slice(first);
        self.tail.extend(rest);
        let excess = self.tail.len().saturating_sub(STDERR_CAPACITY);
        if excess > 0 {
            self.dropped = true;
            self.tail.drain(..excess);
        }
    }

    fn joined(mut self) -> Vec<u8> {
        if self.dropped {
            self.head.extend_from_slice(b"\n...\n");
        }
        self.head.extend(self.tail);
        self.head
    }
}
