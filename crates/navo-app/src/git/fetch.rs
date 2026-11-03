#[path = "fetch_engine.rs"]
mod engine;
#[path = "fetch_ports.rs"]
pub(super) mod ports;

use std::io;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use navo_store::GitSettings;

use crate::git::scan::Cancel;
use engine::{Engine, Step};
pub(crate) use ports::Ports;

const IDLE_WAIT: Duration = Duration::from_secs(1);
const BUSY_WAIT: Duration = Duration::from_millis(100);
const STOP_WAIT: Duration = Duration::from_secs(2);

#[derive(Default)]
struct Wake {
    stopped: Mutex<bool>,
    changed: Condvar,
}

impl Wake {
    fn stop(&self) {
        *self.stopped.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.changed.notify_all();
    }

    fn sleep(&self, timeout: Duration) {
        let stopped = self.stopped.lock().unwrap_or_else(PoisonError::into_inner);
        if !*stopped {
            let _wait = self
                .changed
                .wait_timeout(stopped, timeout)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// Background `git fetch` of the repositories the scanner shows, one thread, one fetch at a time.
pub(crate) struct FetchTimer {
    wake: Arc<Wake>,
    cancel: Cancel,
    running: Option<(JoinHandle<()>, Receiver<()>)>,
}

impl FetchTimer {
    /// Starts nothing unless `[git] fetch` is on.
    pub(crate) fn start(settings: &GitSettings, ports: Ports) -> Option<Self> {
        if !settings.fetch {
            return None;
        }
        let wake = Arc::new(Wake::default());
        let cancel = Cancel::new();
        let (done, finished) = mpsc::sync_channel(1);
        let engine = Engine::new(ports, settings);
        let spawned = thread::Builder::new().name("navo-git-fetch".into()).spawn({
            let (wake, cancel) = (wake.clone(), cancel.clone());
            move || {
                run(engine, &wake, &cancel);
                let _ = done.send(());
            }
        });
        match spawned {
            Ok(thread) => Some(Self {
                wake,
                cancel,
                running: Some((thread, finished)),
            }),
            Err(error) => {
                log_spawn(&error);
                None
            }
        }
    }

    /// Cancels the running fetch and joins the thread; false when it did not stop in time.
    pub(crate) fn stop(&mut self) -> bool {
        let Some((thread, finished)) = self.running.take() else {
            return true;
        };
        self.cancel.cancel();
        self.wake.stop();
        match finished.recv_timeout(STOP_WAIT) {
            Ok(()) => thread.join().is_ok(),
            Err(error) => {
                tracing::warn!(%error, "git fetch timer did not stop cleanly");
                false
            }
        }
    }
}

impl Drop for FetchTimer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn log_spawn(error: &io::Error) {
    tracing::warn!(%error, "the git fetch timer did not start");
}

fn run(mut engine: Engine, wake: &Wake, cancel: &Cancel) {
    while !cancel.is_cancelled() {
        match engine.tick(cancel) {
            Step::Ran => {}
            Step::Busy => wake.sleep(BUSY_WAIT),
            Step::Idle => wake.sleep(IDLE_WAIT),
        }
    }
}

#[cfg(test)]
#[path = "fetch_tests.rs"]
mod tests;
