//! Update runner: one thread drives the machine; each feed call runs on its own short thread.

use crate::{Effect, Event, FeedError, UpdateFeed, UpdateState, Version, step};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// How long a check may run before it counts as offline.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const STOP_WAIT: Duration = Duration::from_secs(1);

enum Msg {
    Event(Event),
    /// An answer of the feed call started as `generation`.
    Answer {
        generation: u64,
        event: Event,
    },
    Stop,
}

struct Running {
    thread: JoinHandle<()>,
    done: Receiver<()>,
}

/// Owns the update thread and the state the UI reads.
pub struct UpdateRunner {
    tx: Sender<Msg>,
    state: Arc<Mutex<UpdateState>>,
    running: Option<Running>,
}

impl UpdateRunner {
    /// Starts the runner; with `auto` an installed copy checks at once.
    pub fn start(feed: Arc<dyn UpdateFeed>, auto: bool) -> Self {
        Self::start_with_timeout(feed, auto, CHECK_TIMEOUT)
    }

    /// Like `start`, with the check deadline given (tests use a short one).
    pub fn start_with_timeout(feed: Arc<dyn UpdateFeed>, auto: bool, timeout: Duration) -> Self {
        let state = Arc::new(Mutex::new(UpdateState::new(feed.kind())));
        let (tx, rx) = mpsc::channel();
        let (done, done_rx) = mpsc::channel();
        let worker = Worker {
            feed,
            state: Arc::clone(&state),
            tx: tx.clone(),
            timeout,
            generation: 0,
            deadline: None,
        };
        let spawned = thread::Builder::new()
            .name("navo-update".into())
            .spawn(move || {
                let _done = done;
                worker.run(&rx, auto);
            });
        let running = match spawned {
            Ok(thread) => Some(Running {
                thread,
                done: done_rx,
            }),
            Err(error) => {
                let error = FeedError::Other(format!("cannot start the update thread: {error}"));
                set_failed(&state, error);
                None
            }
        };
        Self { tx, state, running }
    }

    /// A copy of the current state.
    pub fn snapshot(&self) -> UpdateState {
        lock(&self.state).clone()
    }

    pub fn check_now(&self) {
        let _ = self.tx.send(Msg::Event(Event::CheckNow));
    }

    pub fn set_auto(&self, auto: bool) {
        let _ = self.tx.send(Msg::Event(Event::AutoChanged(auto)));
    }

    /// Stops the thread and waits up to 1 s. A running download is left to finish alone.
    pub fn stop(&mut self) -> bool {
        let Some(running) = self.running.take() else {
            return true;
        };
        let _ = self.tx.send(Msg::Stop);
        match running.done.recv_timeout(STOP_WAIT) {
            Err(RecvTimeoutError::Timeout) => false,
            Ok(()) | Err(RecvTimeoutError::Disconnected) => running.thread.join().is_ok(),
        }
    }
}

impl Drop for UpdateRunner {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Worker {
    feed: Arc<dyn UpdateFeed>,
    state: Arc<Mutex<UpdateState>>,
    tx: Sender<Msg>,
    timeout: Duration,
    /// Counts feed calls; an answer from an older call is dropped.
    generation: u64,
    deadline: Option<Instant>,
}

impl Worker {
    fn run(mut self, rx: &Receiver<Msg>, auto: bool) {
        let current = self.feed.current();
        let staged = self.feed.staged();
        self.apply(Event::Started { auto, staged }, &current);
        loop {
            let msg = match self.deadline {
                None => rx.recv().ok(),
                Some(due) => match rx.recv_timeout(due.saturating_duration_since(Instant::now())) {
                    Ok(msg) => Some(msg),
                    Err(RecvTimeoutError::Timeout) => {
                        self.expire(&current);
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => None,
                },
            };
            match msg {
                None | Some(Msg::Stop) => return,
                Some(Msg::Event(event)) => self.apply(event, &current),
                Some(Msg::Answer { generation, event }) if generation == self.generation => {
                    self.apply(event, &current);
                }
                Some(Msg::Answer { .. }) => {}
            }
        }
    }

    /// The check missed its deadline: fail it and drop its late answer.
    fn expire(&mut self, current: &Version) {
        self.generation += 1;
        self.apply(Event::Checked(Err(FeedError::Offline)), current);
    }

    fn apply(&mut self, event: Event, current: &Version) {
        let (checking, effects) = {
            let mut state = lock(&self.state);
            let (next, effects) = step(&state, event, Instant::now(), current);
            *state = next;
            (*state == UpdateState::Checking, effects)
        };
        if !checking {
            self.deadline = None;
        }
        for effect in effects {
            self.launch(effect, current);
        }
    }

    fn launch(&mut self, effect: Effect, current: &Version) {
        self.generation += 1;
        let generation = self.generation;
        if effect == Effect::Check {
            self.deadline = Some(Instant::now() + self.timeout);
        }
        let failure = match &effect {
            Effect::Check => Event::Checked(Err(spawn_error())),
            Effect::Download(_) => Event::Downloaded(Err(spawn_error())),
        };
        let feed = Arc::clone(&self.feed);
        let tx = self.tx.clone();
        let spawned = thread::Builder::new()
            .name("navo-update-call".into())
            .spawn(move || call(&*feed, effect, generation, &tx));
        if spawned.is_err() {
            self.apply(failure, current);
        }
    }
}

fn spawn_error() -> FeedError {
    FeedError::Other("cannot start an update call".into())
}

/// Runs one feed call and sends its answers back.
fn call(feed: &dyn UpdateFeed, effect: Effect, generation: u64, tx: &Sender<Msg>) {
    let send = |event| {
        let _ = tx.send(Msg::Answer { generation, event });
    };
    match effect {
        Effect::Check => send(Event::Checked(guarded(|| feed.check()))),
        Effect::Download(release) => {
            let result =
                guarded(|| feed.download(&release, &|percent| send(Event::Progress(percent))));
            send(Event::Downloaded(result));
        }
    }
}

/// A panic in the feed becomes an error instead of a call that never answers.
fn guarded<T>(call: impl FnOnce() -> Result<T, FeedError>) -> Result<T, FeedError> {
    catch_unwind(AssertUnwindSafe(call))
        .unwrap_or_else(|_| Err(FeedError::Other("the update call panicked".into())))
}

fn set_failed(state: &Mutex<UpdateState>, error: FeedError) {
    *lock(state) = UpdateState::Failed {
        error,
        retry_after: None,
    };
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;
