use std::io::{self, Read};
use std::path::PathBuf;
use std::process::{Child, ChildStderr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::GitError;

const MAX_STDERR: usize = 64 * 1024;

pub(super) struct Watchdog {
    progress: Option<SyncSender<()>>,
    timed_out: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Watchdog {
    pub(super) fn start(child: Arc<Mutex<Child>>, timeout: Duration) -> Result<Self, GitError> {
        let (progress, receiver) = mpsc::sync_channel(1);
        let timed_out = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&timed_out);
        let handle = std::thread::Builder::new()
            .name("git-batch-watchdog".to_owned())
            .spawn(move || watch(receiver, timeout, child, flag))
            .map_err(|error| GitError::Io {
                path: PathBuf::from("git batch watchdog"),
                kind: error.kind(),
            })?;
        Ok(Self {
            progress: Some(progress),
            timed_out,
            handle: Some(handle),
        })
    }

    pub(super) fn progress(&self) {
        if let Some(progress) = &self.progress {
            let _ = progress.try_send(());
        }
    }

    pub(super) fn timed_out(&self) -> bool {
        self.timed_out.load(Ordering::Acquire)
    }

    pub(super) fn finish(&mut self) {
        self.progress.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.finish();
    }
}

fn watch(
    receiver: mpsc::Receiver<()>,
    timeout: Duration,
    child: Arc<Mutex<Child>>,
    flag: Arc<AtomicBool>,
) {
    loop {
        match receiver.recv_timeout(timeout) {
            Ok(()) => {}
            Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => {
                flag.store(true, Ordering::Release);
                kill(&child);
                return;
            }
        }
    }
}

fn kill(child: &Arc<Mutex<Child>>) {
    if let Ok(mut child) = child.lock() {
        let _ = child.kill();
    }
}

pub(super) fn capture_stderr(mut stderr: ChildStderr, buffer: Arc<Mutex<Vec<u8>>>) {
    let mut chunk = [0u8; 4096];
    loop {
        match stderr.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                if let Ok(mut buffer) = buffer.lock() {
                    append(&mut buffer, &chunk[..read]);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
}

fn append(buffer: &mut Vec<u8>, chunk: &[u8]) {
    buffer.extend_from_slice(chunk);
    if buffer.len() > MAX_STDERR {
        let excess = buffer.len() - MAX_STDERR;
        buffer.drain(..excess);
    }
}
