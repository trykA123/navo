use std::io::{self, Read};
use std::process::Child;
use std::sync::{Arc, Mutex};

use crate::GitError;

use super::Batch;
use super::watchdog::Watchdog;

const DRAIN_LIMIT: u64 = 1024 * 1024;

pub(in crate::cli) struct StartedBlob {
    size: u64,
    watchdog: Watchdog,
    generation: u64,
}

impl StartedBlob {
    pub(super) fn new(size: u64, watchdog: Watchdog, generation: u64) -> Self {
        Self {
            size,
            watchdog,
            generation,
        }
    }
}

pub(in crate::cli) struct BlobStream {
    session: Arc<Mutex<Option<Batch>>>,
    generation: u64,
    remaining: u64,
    finished: bool,
    watchdog: Option<Watchdog>,
}

impl BlobStream {
    pub(in crate::cli) fn new(session: Arc<Mutex<Option<Batch>>>, started: StartedBlob) -> Self {
        let finished = started.size == 0;
        Self {
            session,
            generation: started.generation,
            remaining: started.size,
            finished,
            watchdog: (!finished).then_some(started.watchdog),
        }
    }
}

impl Read for BlobStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.finished || buffer.is_empty() {
            return Ok(0);
        }
        let Some(watchdog) = self.watchdog.as_ref() else {
            return Err(broken_pipe("the git cat-file request ended"));
        };
        let mut slot = self
            .session
            .lock()
            .map_err(|_| io::Error::other("the git cat-file session was poisoned"))?;
        let Some(batch) = slot.as_mut() else {
            return Err(broken_pipe("the git cat-file session closed"));
        };
        if batch.dead || batch.generation() != self.generation {
            return Err(broken_pipe("the git cat-file session closed"));
        }
        let want = buffer
            .len()
            .min(usize::try_from(self.remaining).unwrap_or(usize::MAX));
        let read = read_from_batch(batch, watchdog, &mut buffer[..want])?;
        self.remaining = self
            .remaining
            .saturating_sub(u64::try_from(read).unwrap_or(u64::MAX));
        if self.remaining == 0 {
            finish_request(batch, watchdog)?;
            self.finished = true;
            self.watchdog = None;
        }
        Ok(read)
    }
}

fn read_from_batch(batch: &mut Batch, watchdog: &Watchdog, buffer: &mut [u8]) -> io::Result<usize> {
    match batch.stdout.read(buffer) {
        Ok(0) => {
            batch.dead = true;
            if watchdog.timed_out() {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "git timed out"));
            }
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the git cat-file blob ended early",
            ))
        }
        Ok(read) => {
            watchdog.progress();
            Ok(read)
        }
        Err(error) => {
            batch.dead = true;
            if watchdog.timed_out() {
                Err(io::Error::new(io::ErrorKind::TimedOut, "git timed out"))
            } else {
                Err(error)
            }
        }
    }
}

fn finish_request(batch: &mut Batch, watchdog: &Watchdog) -> io::Result<()> {
    batch.finish_blob(watchdog).map_err(|error| match error {
        GitError::Timeout => io::Error::new(io::ErrorKind::TimedOut, "git timed out"),
        other => io::Error::other(other),
    })
}

fn broken_pipe(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, message)
}

impl Drop for BlobStream {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let watchdog = self.watchdog.take();
        if let Ok(mut slot) = self.session.lock() {
            finish_drop(
                &mut slot,
                self.generation,
                self.remaining,
                watchdog.as_ref(),
            );
        }
    }
}

fn finish_drop(
    slot: &mut Option<Batch>,
    generation: u64,
    remaining: u64,
    watchdog: Option<&Watchdog>,
) {
    let Some(batch) = slot.as_mut() else {
        return;
    };
    if batch.generation() != generation {
        return;
    }
    if remaining <= DRAIN_LIMIT
        && let Some(watchdog) = watchdog
        && batch.drain_blob(remaining, watchdog).is_ok()
    {
        return;
    }
    batch.abort_blob();
    *slot = None;
}

pub(super) fn child_status(child: &Arc<Mutex<Child>>) -> Option<i32> {
    let mut child = child.lock().ok()?;
    match child.try_wait() {
        Ok(Some(status)) => status.code(),
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            None
        }
    }
}

pub(super) fn timed_out(watchdog: &Watchdog) -> Result<(), GitError> {
    if watchdog.timed_out() {
        Err(GitError::Timeout)
    } else {
        Ok(())
    }
}
