use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{GitError, ObjectId};

mod stream;
mod util;
mod watchdog;

pub(in crate::cli) use stream::BlobStream;

use stream::{StartedBlob, child_status, timed_out};
use util::{hex, io_error, parse_header, stop_child, stop_locked};
use watchdog::{Watchdog, capture_stderr};

const MAX_HEADER: usize = 1024;

pub(super) struct Batch {
    child: Arc<Mutex<Child>>,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    stderr: Arc<Mutex<Vec<u8>>>,
    stderr_thread: Option<std::thread::JoinHandle<()>>,
    repo: PathBuf,
    generation: u64,
    dead: bool,
    pending: bool,
}

impl Batch {
    pub(super) fn new(mut child: Child, repo: PathBuf, generation: u64) -> Result<Self, GitError> {
        let Some(stdin) = child.stdin.take() else {
            stop_child(&mut child);
            return Err(GitError::Protocol {
                detail: "cat-file stdin was not available".to_owned(),
            });
        };
        let Some(stdout) = child.stdout.take() else {
            stop_child(&mut child);
            return Err(GitError::Protocol {
                detail: "cat-file stdout was not available".to_owned(),
            });
        };
        let Some(stderr) = child.stderr.take() else {
            stop_child(&mut child);
            return Err(GitError::Protocol {
                detail: "cat-file stderr was not available".to_owned(),
            });
        };
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let capture = Arc::clone(&buffer);
        let thread = match std::thread::Builder::new()
            .name("git-batch-stderr".to_owned())
            .spawn(move || capture_stderr(stderr, capture))
        {
            Ok(thread) => thread,
            Err(error) => {
                stop_child(&mut child);
                return Err(GitError::Io {
                    path: PathBuf::from("git cat-file"),
                    kind: error.kind(),
                });
            }
        };
        Ok(Self {
            child: Arc::new(Mutex::new(child)),
            stdin,
            stdout: BufReader::new(stdout),
            stderr: buffer,
            stderr_thread: Some(thread),
            repo,
            generation,
            dead: false,
            pending: false,
        })
    }

    pub(super) fn start_blob(
        &mut self,
        id: &ObjectId,
        timeout: Duration,
    ) -> Result<StartedBlob, GitError> {
        self.ready()?;
        let watchdog = Watchdog::start(Arc::clone(&self.child), timeout)?;
        if self.write_command(id).is_err() {
            self.dead = true;
            if watchdog.timed_out() {
                return Err(GitError::Timeout);
            }
            return Err(self.session_error());
        }
        let header = match self.read_header(&watchdog) {
            Ok(header) => header,
            Err(error) => {
                self.dead = true;
                if watchdog.timed_out() {
                    return Err(GitError::Timeout);
                }
                if matches!(error, GitError::SessionClosed) {
                    return Err(self.session_error());
                }
                return Err(error);
            }
        };
        if header == b"missing" || header.ends_with(b" missing") {
            return Err(GitError::MissingObject { id: *id });
        }
        let (kind, size) = match parse_header(&header) {
            Ok(parsed) => parsed,
            Err(error) => {
                self.dead = true;
                return Err(error);
            }
        };
        if kind != b"blob" {
            self.dead = true;
            return Err(GitError::Unsupported {
                operation: format!("reading a {} as a blob", String::from_utf8_lossy(kind)),
            });
        }
        self.pending = true;
        if size == 0 {
            self.finish_blob(&watchdog)?;
        }
        Ok(StartedBlob::new(size, watchdog, self.generation))
    }

    fn write_command(&mut self, id: &ObjectId) -> io::Result<()> {
        self.stdin
            .write_all(format!("contents {}\nflush\n", hex(id)).as_bytes())?;
        self.stdin.flush()
    }

    fn read_header(&mut self, watchdog: &Watchdog) -> Result<Vec<u8>, GitError> {
        let mut line = Vec::new();
        loop {
            let available = self.stdout.fill_buf().map_err(|error| {
                self.dead = true;
                io_error(error)
            })?;
            if available.is_empty() {
                self.dead = true;
                return Err(GitError::SessionClosed);
            }
            let found = available.iter().position(|byte| *byte == b'\n');
            let take = found.map_or(available.len(), |position| position + 1);
            if line.len().saturating_add(take) > MAX_HEADER {
                self.dead = true;
                return Err(GitError::Protocol {
                    detail: "cat-file header was too long".to_owned(),
                });
            }
            let chunk = available[..take].to_vec();
            self.stdout.consume(take);
            line.extend_from_slice(&chunk);
            watchdog.progress();
            if found.is_some() {
                line.pop();
                return Ok(line);
            }
        }
    }

    fn finish_blob(&mut self, watchdog: &Watchdog) -> Result<(), GitError> {
        let mut newline = [0u8; 1];
        if let Err(error) = self.stdout.read_exact(&mut newline) {
            self.dead = true;
            timed_out(watchdog)?;
            return Err(io_error(error));
        }
        if newline != *b"\n" {
            self.dead = true;
            return Err(GitError::Protocol {
                detail: "cat-file blob was not followed by a newline".to_owned(),
            });
        }
        self.pending = false;
        watchdog.progress();
        Ok(())
    }

    fn drain_blob(&mut self, mut remaining: u64, watchdog: &Watchdog) -> Result<(), GitError> {
        let mut buffer = [0u8; 8 * 1024];
        while remaining > 0 {
            let want = buffer
                .len()
                .min(usize::try_from(remaining).unwrap_or(usize::MAX));
            match self.stdout.read(&mut buffer[..want]) {
                Ok(0) => {
                    self.dead = true;
                    timed_out(watchdog)?;
                    return Err(self.session_error());
                }
                Ok(read) => {
                    watchdog.progress();
                    remaining = remaining.saturating_sub(u64::try_from(read).unwrap_or(u64::MAX))
                }
                Err(error) => {
                    self.dead = true;
                    timed_out(watchdog)?;
                    return Err(io_error(error));
                }
            }
        }
        self.finish_blob(watchdog)
    }

    fn abort_blob(&mut self) {
        self.pending = false;
        self.dead = true;
        stop_locked(&self.child);
    }

    pub(super) fn is_dead(&self) -> bool {
        self.dead
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    fn ready(&self) -> Result<(), GitError> {
        if self.dead {
            return Err(GitError::SessionClosed);
        }
        if self.pending {
            return Err(GitError::SessionBusy);
        }
        Ok(())
    }

    fn session_error(&mut self) -> GitError {
        let status = child_status(&self.child);
        if let Some(thread) = self.stderr_thread.take() {
            let _ = thread.join();
        }
        let stderr = match self.stderr.lock() {
            Ok(stderr) => String::from_utf8_lossy(&stderr).trim().to_owned(),
            Err(_) => String::new(),
        };
        if stderr.contains("detected dubious ownership") {
            return GitError::UnsafeDirectory {
                path: self.repo.clone(),
            };
        }
        GitError::Command {
            args: "cat-file --batch-command --buffer".to_owned(),
            status,
            stderr,
        }
    }
}

impl Drop for Batch {
    fn drop(&mut self) {
        stop_locked(&self.child);
        if let Some(thread) = self.stderr_thread.take() {
            let _ = thread.join();
        }
    }
}
