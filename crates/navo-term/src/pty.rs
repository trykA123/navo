//! The real backend: a `portable-pty` session (ConPTY on Windows). A reader thread drains output
//! into the sink, a waiter thread reaps the child; drop kills the child and closes off-thread.

use std::error::Error;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use portable_pty::{Child, ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::backend::{Backend, Sink};
use crate::shells::ShellSpec;
use crate::{Size, TermError};

/// The PTY master, shared with the waiter so it can close the console once the child exits
/// (ConPTY keeps the output pipe open until then).
type SharedMaster = Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>;

const READ_CHUNK: usize = 16 * 1024;
/// R2: how long a dropped session waits for its reader before detaching it.
const READER_LIMIT: Duration = Duration::from_secs(2);

/// A shell running in a pseudo terminal.
pub struct PtyBackend {
    master: SharedMaster,
    writer: Option<Box<dyn Write + Send>>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    reader: Option<JoinHandle<()>>,
    reader_done: Option<Receiver<()>>,
}

impl PtyBackend {
    /// Starts `shell` in `cwd` (the home folder if `cwd` is gone); output goes to `sink`.
    pub fn spawn(
        shell: &ShellSpec,
        cwd: &Path,
        size: Size,
        sink: Arc<dyn Sink>,
    ) -> Result<Self, TermError> {
        size.check()?;
        if shell.program.is_absolute() && !shell.program.is_file() {
            return Err(TermError::ShellMissing(shell.label.clone()));
        }
        let pair = native_pty_system()
            .openpty(pty_size(size))
            .map_err(|error| TermError::Io(os_error(&*error)))?;
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| TermError::Io(os_error(&*error)))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| TermError::Io(os_error(&*error)))?;
        let child = pair
            .slave
            .spawn_command(command(shell, cwd))
            .map_err(|error| spawn_error(shell, &*error))?;
        drop(pair.slave);
        let mut killer = child.clone_killer();
        let master: SharedMaster = Arc::new(Mutex::new(Some(pair.master)));
        let started = start_threads(child, reader, &master, sink);
        let (reader, reader_done) = started.inspect_err(|_| {
            if let Err(error) = killer.kill() {
                tracing::warn!(%error, "terminal child not killed after a failed start");
            }
        })?;
        Ok(Self {
            master,
            writer: Some(writer),
            killer,
            reader: Some(reader),
            reader_done: Some(reader_done),
        })
    }
}

impl Backend for PtyBackend {
    fn write(&mut self, bytes: &[u8]) -> Result<(), TermError> {
        let writer = self.writer.as_mut().ok_or(TermError::Exited)?;
        writer
            .write_all(bytes)
            .and_then(|()| writer.flush())
            .map_err(TermError::Io)
    }

    fn resize(&mut self, size: Size) -> Result<(), TermError> {
        size.check()?;
        let master = self.master.lock().unwrap_or_else(PoisonError::into_inner);
        let master = master.as_ref().ok_or(TermError::Exited)?;
        master
            .resize(pty_size(size))
            .map_err(|error| TermError::Io(os_error(&*error)))
    }
}

impl Drop for PtyBackend {
    /// R2's close order: kill the child, then on a helper thread close input, close the
    /// console (which can block until output is drained) and give the reader 2 s to finish.
    fn drop(&mut self) {
        if let Err(error) = self.killer.kill() {
            tracing::debug!(%error, "terminal child already gone");
        }
        let writer = self.writer.take();
        let master = Arc::clone(&self.master);
        let reader = self.reader.take();
        let reader_done = self.reader_done.take();
        let closer = thread::Builder::new()
            .name(String::from("navo-term-close"))
            .spawn(move || {
                drop(writer);
                close(&master);
                finish_reader(reader, reader_done);
            });
        if let Err(error) = closer {
            tracing::warn!(%error, "terminal close thread did not start; reader detached");
        }
    }
}

fn command(shell: &ShellSpec, cwd: &Path) -> CommandBuilder {
    let mut command = CommandBuilder::new(&shell.program);
    command.args(&shell.args);
    command.cwd(cwd);
    if !cfg!(windows) {
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
    }
    command
}

/// Starts the waiter (reaps the child, then closes the console) and the reader (drains
/// output, then reports the waiter's exit code). The reader signals `done` last.
fn start_threads(
    mut child: Box<dyn Child + Send + Sync>,
    mut output: Box<dyn Read + Send>,
    master: &SharedMaster,
    sink: Arc<dyn Sink>,
) -> Result<(JoinHandle<()>, Receiver<()>), TermError> {
    let waiter_master = Arc::clone(master);
    let waiter = thread::Builder::new()
        .name(String::from("navo-term-wait"))
        .spawn(move || {
            let code = child.wait().ok().map(|status| status.exit_code());
            close(&waiter_master);
            code
        })
        .map_err(TermError::Io)?;
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let reader = thread::Builder::new()
        .name(String::from("navo-term-read"))
        .spawn(move || {
            drain(&mut *output, &*sink);
            sink.exited(waiter.join().ok().flatten());
            // The receiver is gone only when the session stopped waiting; nothing to tell.
            let _ = done_tx.send(());
        })
        .map_err(TermError::Io)?;
    Ok((reader, done_rx))
}

/// Reads until EOF or a read error (a closed PTY reports EIO on Unix).
fn drain(output: &mut dyn Read, sink: &dyn Sink) {
    let mut buffer = vec![0; READ_CHUNK];
    loop {
        match output.read(&mut buffer) {
            Ok(0) => return,
            Ok(read) => sink.output(buffer.get(..read).unwrap_or_default()),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => {
                tracing::debug!(%error, "terminal output closed");
                return;
            }
        }
    }
}

/// Drops the master outside the lock: on Windows this is `ClosePseudoConsole`, which can block.
fn close(master: &SharedMaster) {
    let taken = master.lock().unwrap_or_else(PoisonError::into_inner).take();
    drop(taken);
}

fn finish_reader(reader: Option<JoinHandle<()>>, done: Option<Receiver<()>>) {
    let (Some(reader), Some(done)) = (reader, done) else {
        return;
    };
    match done.recv_timeout(READER_LIMIT) {
        Ok(()) | Err(RecvTimeoutError::Disconnected) => {
            if reader.join().is_err() {
                tracing::warn!("terminal reader panicked");
            }
        }
        Err(RecvTimeoutError::Timeout) => {
            tracing::warn!("terminal reader still running after 2 s; detached");
        }
    }
}

fn pty_size(size: Size) -> PtySize {
    PtySize {
        rows: size.rows,
        cols: size.cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// A missing program is `ShellMissing`; any other failure keeps its OS error kind.
fn spawn_error(shell: &ShellSpec, error: &(dyn Error + 'static)) -> TermError {
    let error = os_error(error);
    match error.kind() {
        io::ErrorKind::NotFound => TermError::ShellMissing(shell.label.clone()),
        _ => TermError::Spawn(error),
    }
}

/// Finds the `io::Error` in an error chain and keeps its kind with the full message.
fn os_error(error: &(dyn Error + 'static)) -> io::Error {
    let kind = std::iter::successors(Some(error), |&inner| inner.source())
        .find_map(|inner| inner.downcast_ref::<io::Error>())
        .map_or(io::ErrorKind::Other, io::Error::kind);
    io::Error::new(kind, error.to_string())
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::os_error;

    #[derive(Debug, thiserror::Error)]
    #[error("spawn failed")]
    struct Wrapped(#[source] io::Error);

    #[test]
    fn spawn_errors_keep_the_os_error_kind_from_the_chain() {
        let denied = Wrapped(io::Error::from(io::ErrorKind::PermissionDenied));
        assert_eq!(os_error(&denied).kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(
            os_error(&Wrapped(io::Error::other("x"))).kind(),
            io::ErrorKind::Other
        );
    }
}
