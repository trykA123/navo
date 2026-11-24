//! The one owner of the shell process and its console. Only this thread signals, reaps and
//! resizes, so it never signals a reaped pid and never shares the console behind a lock.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use portable_pty::{Child, MasterPty};

use super::output::Outbox;
use super::{pty_size, run_thread};
use crate::{Size, TermError};

/// How often the owner checks whether the shell exited on its own.
const POLL: Duration = Duration::from_millis(50);

/// The latest size the UI asked for; the owner takes it, so bursts coalesce to the last one.
type PendingSize = Arc<Mutex<Option<Size>>>;

/// The shell and its console, as `portable-pty` hands them over.
pub(super) type Process = (Box<dyn Child + Send + Sync>, Box<dyn MasterPty + Send>);

/// The session's side of the owner. Dropping it asks the owner to end the shell.
pub(super) struct Control {
    wake: SyncSender<()>,
    pending: PendingSize,
}

impl Control {
    /// Queues `size` for the owner thread; never waits for the system call.
    pub(super) fn resize(&self, size: Size) -> Result<(), TermError> {
        *self.pending.lock().unwrap_or_else(PoisonError::into_inner) = Some(size);
        match self.wake.try_send(()) {
            // Full: a wake is already queued and will pick up the new size.
            Ok(()) | Err(TrySendError::Full(())) => Ok(()),
            Err(TrySendError::Disconnected(())) => Err(TermError::Exited),
        }
    }
}

/// How the owner's watch ended.
enum End {
    /// The shell exited by itself.
    Exited,
    /// The session was dropped: end the shell.
    Stop,
    /// The exit status could not be read; signalling could hit a recycled pid.
    Lost,
}

/// Owns the shell and the console for the session's whole life.
struct Owner {
    child: Box<dyn Child + Send + Sync>,
    master: Option<Box<dyn MasterPty + Send>>,
    wake: Receiver<()>,
    pending: PendingSize,
    outbox: Arc<Outbox>,
}

/// Starts the owner thread. If it cannot start, ends the shell and closes the console here;
/// the sink then hears nothing.
pub(super) fn start(process: Process, outbox: &Arc<Outbox>) -> std::io::Result<Control> {
    let (wake_tx, wake) = mpsc::sync_channel(1);
    let pending = PendingSize::default();
    let owner = Owner::new(process, wake, Arc::clone(&pending), outbox);
    match run_thread("navo-term-child", owner, Owner::run) {
        Ok(_) => Ok(Control {
            wake: wake_tx,
            pending,
        }),
        Err((error, owner)) => {
            outbox.close();
            if let Some(owner) = owner {
                owner.finish(&End::Stop);
            }
            Err(error)
        }
    }
}

/// Ends and reaps a shell whose session failed to start, then closes its console; the sink
/// hears nothing. Runs on
/// the caller's thread (Unix: up to the 200 ms SIGHUP grace); only thread-start failures
/// get here.
pub(super) fn abandon(process: Process, outbox: &Arc<Outbox>) {
    outbox.close();
    let (_, wake) = mpsc::sync_channel(1);
    Owner::new(process, wake, PendingSize::default(), outbox).finish(&End::Stop);
}

impl Owner {
    fn new(
        (child, master): Process,
        wake: Receiver<()>,
        pending: PendingSize,
        outbox: &Arc<Outbox>,
    ) -> Self {
        Self {
            child,
            master: Some(master),
            wake,
            pending,
            outbox: Arc::clone(outbox),
        }
    }

    fn run(mut self) {
        let end = loop {
            match self.wake.recv_timeout(POLL) {
                Ok(()) => self.apply_size(),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break End::Stop,
            }
            match self.child.try_wait() {
                Ok(Some(_)) => break End::Exited,
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(%error, "terminal shell status unreadable");
                    break End::Lost;
                }
            }
        };
        self.finish(&end);
    }

    fn apply_size(&mut self) {
        let size = self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let (Some(size), Some(master)) = (size, &self.master)
            && let Err(error) = master.resize(pty_size(size))
        {
            tracing::warn!(%error, "terminal resize failed");
        }
    }

    /// Ends the shell when asked, reaps it, reports the exit (which starts the reader's
    /// limit), then closes the console so ConPTY's reader sees EOF. Closing can block on
    /// older Windows until output is drained; the reader keeps draining meanwhile.
    ///
    /// `kill` is `portable-pty`'s: on Unix SIGHUP, then SIGKILL if the shell is still alive
    /// after 200 ms; on Windows `TerminateProcess`. The child is not reaped before this, so
    /// its pid cannot have been reused.
    fn finish(mut self, end: &End) {
        if matches!(end, End::Stop)
            && let Err(error) = self.child.kill()
        {
            tracing::debug!(%error, "terminal shell already gone");
        }
        let code = match end {
            End::Lost => None,
            End::Exited | End::Stop => match self.child.wait() {
                Ok(status) => Some(status.exit_code()),
                Err(error) => {
                    tracing::warn!(%error, "terminal shell not reaped");
                    None
                }
            },
        };
        self.outbox.exited(code);
        drop(self.master.take());
    }
}
