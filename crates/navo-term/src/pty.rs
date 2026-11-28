//! The real backend: a `portable-pty` session (ConPTY on Windows). Four threads do the work:
//! one owner ends, reaps and resizes the shell, a reader drains output, a sink thread hands
//! it to the `Sink`, and a writer feeds input. The caller never waits on any of them. On
//! Windows the reader answers ConPTY's start-up cursor query through the writer.

mod child;
mod handshake;
mod input;
mod output;

use std::error::Error;
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use crate::backend::{Backend, Sink};
use crate::prompt::{Nonce, PromptSink, PtyWriter, ShellHook};
use crate::shells::ShellSpec;
use crate::{Size, TermError};

use output::Outbox;

/// A shell running in a pseudo terminal. Dropping it asks the owner to end the shell.
pub struct PtyBackend {
    input: input::Input,
    control: child::Control,
    /// Only the Unix close tests read it.
    #[cfg(all(test, unix))]
    outbox: Arc<Outbox>,
}

impl PtyBackend {
    /// Starts `shell` in `cwd`; output and then the exit go to `sink`.
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
        // portable-pty silently starts in the home folder when `cwd` is gone; refuse instead.
        if !cwd.is_dir() {
            return Err(TermError::CwdMissing(cwd.to_path_buf()));
        }
        let child = pair
            .slave
            .spawn_command(command(shell, cwd, None))
            .map_err(|error| spawn_error(shell, cwd, &*error))?;
        drop(pair.slave);
        let process = (child, pair.master);
        let outbox = Arc::new(Outbox::default());
        // Each step that fails ends and reaps the shell; the read handle is gone by then.
        let started = input::start(writer).and_then(|input| {
            let replies = cfg!(windows).then(|| input.replies());
            output::read(reader, handshake::Handshake::new(replies), &outbox)
                .and_then(|()| output::deliver(&outbox, sink))
                .map(|()| input)
        });
        let input = match started {
            Ok(input) => input,
            Err(error) => {
                child::abandon(process, &outbox);
                return Err(TermError::Io(error));
            }
        };
        let control = child::start(process, &outbox).map_err(TermError::Io)?;
        Ok(Self {
            input,
            control,
            #[cfg(all(test, unix))]
            outbox,
        })
    }

    /// Starts `shell` with an optional prompt hook, wired to one `PtyWriter` and `PromptSink`.
    pub(crate) fn spawn_session(
        shell: &ShellSpec,
        cwd: &Path,
        size: Size,
        sink: Arc<dyn Sink>,
        nonce: Option<Nonce>,
        hook: Option<&ShellHook>,
    ) -> Result<Arc<PtyWriter>, TermError> {
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
        if !cwd.is_dir() {
            return Err(TermError::CwdMissing(cwd.to_path_buf()));
        }
        let child = pair
            .slave
            .spawn_command(command(shell, cwd, hook))
            .map_err(|error| spawn_error(shell, cwd, &*error))?;
        drop(pair.slave);
        let process = (child, pair.master);
        let outbox = Arc::new(Outbox::default());
        let started = input::start(writer).and_then(|input| {
            let replies = cfg!(windows).then(|| input.replies());
            output::read(reader, handshake::Handshake::new(replies), &outbox).map(|()| input)
        });
        let input = match started {
            Ok(input) => input,
            Err(error) => {
                child::abandon(process, &outbox);
                return Err(TermError::Io(error));
            }
        };
        let control = child::start(process, &outbox).map_err(TermError::Io)?;
        let backend = Self {
            input,
            control,
            #[cfg(all(test, unix))]
            outbox: Arc::clone(&outbox),
        };
        let pty_writer = Arc::new(PtyWriter::new(shell.kind, nonce, Box::new(backend)));
        if let Some(init) = hook.and_then(ShellHook::init) {
            pty_writer.attach_init(init);
        }
        let prompt_sink = Arc::new(PromptSink::for_writer(&pty_writer, sink));
        if let Err(error) = output::deliver(&outbox, prompt_sink) {
            outbox.close();
            drop(pty_writer);
            return Err(TermError::Io(error));
        }
        Ok(pty_writer)
    }
}

impl Backend for PtyBackend {
    fn write(&mut self, bytes: &[u8]) -> Result<(), TermError> {
        self.input.send(bytes)
    }

    fn resize(&mut self, size: Size) -> Result<(), TermError> {
        size.check()?;
        self.control.resize(size)
    }
}

#[cfg(all(test, unix))]
impl PtyBackend {
    /// Whether the reader finished within `limit`.
    pub(crate) fn reader_ends_within(&self, limit: std::time::Duration) -> bool {
        self.outbox.ended_within(limit)
    }
}

fn command(shell: &ShellSpec, cwd: &Path, hook: Option<&ShellHook>) -> CommandBuilder {
    let mut command = CommandBuilder::new(&shell.program);
    // Git Bash's hook replaces `--login -i`: bash ignores --init-file in a login shell.
    if !hook.is_some_and(ShellHook::replaces_args) {
        command.args(&shell.args);
    }
    if let Some(hook) = hook {
        command.args(hook.args());
        for (key, value) in hook.env() {
            command.env(key, value);
        }
    }
    command.cwd(cwd);
    if !cfg!(windows) {
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
    }
    command
}

/// Runs `body(state)` on a named thread. If the thread cannot start, hands `state` back so
/// the caller can clean up what it owns (`std` would drop it with the closure).
fn run_thread<T, R>(
    name: &str,
    state: T,
    body: impl FnOnce(T) -> R + Send + 'static,
) -> Result<JoinHandle<Option<R>>, (io::Error, Option<T>)>
where
    T: Send + 'static,
    R: Send + 'static,
{
    let slot = Arc::new(Mutex::new(Some(state)));
    let theirs = Arc::clone(&slot);
    let spawned = thread::Builder::new()
        .name(String::from(name))
        .spawn(move || {
            let state = theirs.lock().unwrap_or_else(PoisonError::into_inner).take();
            state.map(body)
        });
    spawned.map_err(|error| {
        let state = slot.lock().unwrap_or_else(PoisonError::into_inner).take();
        (error, state)
    })
}

fn pty_size(size: Size) -> PtySize {
    PtySize {
        rows: size.rows,
        cols: size.cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// `CwdMissing` when the start folder vanished meanwhile, `ShellMissing` for a program that
/// does not exist, otherwise `Spawn` with the OS error kind.
fn spawn_error(shell: &ShellSpec, cwd: &Path, error: &(dyn Error + 'static)) -> TermError {
    if !cwd.is_dir() {
        return TermError::CwdMissing(cwd.to_path_buf());
    }
    let error = os_error(error);
    match error.kind() {
        io::ErrorKind::NotFound => TermError::ShellMissing(shell.label.clone()),
        _ => TermError::Spawn(error),
    }
}

/// Keeps the full message with the most precise kind: an `io::Error` in the chain, else the
/// `(os error N)` that portable-pty formats into Windows `CreateProcessW` failures, else its
/// Unix program lookup messages.
fn os_error(error: &(dyn Error + 'static)) -> io::Error {
    let message = error.to_string();
    let kind = std::iter::successors(Some(error), |&inner| inner.source())
        .find_map(|inner| inner.downcast_ref::<io::Error>())
        .map(io::Error::kind)
        .or_else(|| raw_os_error(&message).map(|code| io::Error::from_raw_os_error(code).kind()))
        .unwrap_or_else(|| lookup_kind(&message));
    io::Error::new(kind, message)
}

fn raw_os_error(message: &str) -> Option<i32> {
    let (_, rest) = message.rsplit_once("(os error ")?;
    rest.split_once(')')?.0.parse().ok()
}

fn lookup_kind(message: &str) -> io::ErrorKind {
    let missing = ["does not exist", "doesn't exist", "No viable candidates"];
    if missing.iter().any(|text| message.contains(text)) {
        io::ErrorKind::NotFound
    } else if message.contains("not executable") {
        io::ErrorKind::PermissionDenied
    } else {
        io::ErrorKind::Other
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::os_error;

    #[derive(Debug, thiserror::Error)]
    #[error("spawn failed")]
    struct Wrapped(#[source] io::Error);

    #[derive(Debug, thiserror::Error)]
    #[error("{0}")]
    struct Flat(String);

    fn kind_of(message: &str) -> io::ErrorKind {
        os_error(&Flat(String::from(message))).kind()
    }

    #[test]
    fn spawn_errors_keep_the_os_error_kind_from_the_chain() {
        let denied = Wrapped(io::Error::from(io::ErrorKind::PermissionDenied));
        assert_eq!(os_error(&denied).kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(
            os_error(&Wrapped(io::Error::other("x"))).kind(),
            io::ErrorKind::Other
        );
    }

    #[test]
    fn flattened_spawn_errors_get_their_kind_back_from_the_message() {
        // 2 is "not found" on both Unix (ENOENT) and Windows (ERROR_FILE_NOT_FOUND).
        let windows = "CreateProcessW `x` in cwd `None` failed: gone (os error 2)";
        assert_eq!(kind_of(windows), io::ErrorKind::NotFound);
        let lookup = "Unable to spawn x because:\nNo viable candidates found in PATH";
        assert_eq!(kind_of(lookup), io::ErrorKind::NotFound);
        let denied = "Unable to spawn /x because it is not executable (EACCES)";
        assert_eq!(kind_of(denied), io::ErrorKind::PermissionDenied);
        assert_eq!(kind_of("something else"), io::ErrorKind::Other);
    }
}
