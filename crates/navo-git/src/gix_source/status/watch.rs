use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{Builder, JoinHandle};
use std::time::{Duration, Instant};

use crate::GitError;
use crate::cli::Cancel;

const POLL: Duration = Duration::from_millis(10);

/// Raises a flag when `cancel` is set or the deadline passes, so gix workers stop.
pub(super) struct Watch {
    stop: Arc<AtomicBool>,
    cancel: Cancel,
    deadline: Instant,
    ended: Option<Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

impl Watch {
    pub(super) fn start(cancel: &Cancel, timeout: Duration) -> io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let deadline = Instant::now() + timeout;
        let (ended, ending) = mpsc::channel::<()>();
        let (flag, watched) = (Arc::clone(&stop), cancel.clone());
        let handle = Builder::new()
            .name("gix-status-watch".to_owned())
            .spawn(move || poll(&watched, &flag, deadline, &ending))?;
        Ok(Self {
            stop,
            cancel: cancel.clone(),
            deadline,
            ended: Some(ended),
            handle: Some(handle),
        })
    }

    pub(super) fn flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.stop)
    }

    pub(super) fn stopped(&self) -> &AtomicBool {
        &self.stop
    }

    /// The error that ended the run early, if any; a cancel wins over a timeout.
    pub(super) fn interruption(&self) -> Option<GitError> {
        if self.cancel.is_cancelled() {
            Some(GitError::Cancelled)
        } else if self.stop.load(Ordering::Acquire) || Instant::now() >= self.deadline {
            Some(GitError::Timeout)
        } else {
            None
        }
    }
}

fn poll(cancel: &Cancel, flag: &AtomicBool, deadline: Instant, ending: &Receiver<()>) {
    while !cancel.is_cancelled() && Instant::now() < deadline {
        if !matches!(ending.recv_timeout(POLL), Err(RecvTimeoutError::Timeout)) {
            return;
        }
    }
    flag.store(true, Ordering::Release);
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.ended.take();
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap_or_default();
        }
    }
}
