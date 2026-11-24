//! How the client starts a helper: the program and arguments, an optional spawn hook, and the
//! default spawn with `Command` (S2a; `navo-app` plugs a confined spawn into the hook).

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;

use super::process::{HelperProcess, Spawned};

/// The helper program and its arguments, handed to the spawn hook.
#[derive(Clone, Debug)]
pub struct HelperSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
}

/// Starts a helper from a spec. Contract: a hook that returns `Err` has no live child left. A hook
/// that cannot confine the helper returns `io::Error::other(Unconfined { .. })`; one that found a
/// helper file missing returns `io::Error::other(PdfError::NoEngine(..))`, reported as is.
pub type Spawn = Arc<dyn Fn(&HelperSpec) -> io::Result<Spawned> + Send + Sync>;

/// Whether a confinement failure may clear on a later try (a lock, low memory) or never will.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permanence {
    Transient,
    Permanent,
}

/// A spawn hook's refusal: it could not confine the helper, so it started nothing. The client
/// reports `PdfError::Unconfined`; after a `Permanent` one it never calls the hook again.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
pub struct Unconfined {
    pub reason: String,
    pub permanence: Permanence,
}

impl HelperSpec {
    /// A `Command` for this helper with piped stdin and stdout and a null stderr.
    #[must_use]
    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        command
    }

    /// The default spawn: runs `command()` with no guard. Fails when the program cannot start.
    pub fn spawn(&self) -> io::Result<Spawned> {
        Spawned::from_child(self.command().spawn()?, None)
    }
}

impl Spawned {
    /// Wraps a child started with piped stdin and stdout, keeping `guard` beside it. When a pipe
    /// is missing it kills and reaps the child, then fails, so no live child is left.
    pub fn from_child(mut child: Child, guard: Option<Box<dyn Send>>) -> io::Result<Self> {
        let (Some(input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = HelperProcess::kill(&mut child);
            let _ = child.wait();
            return Err(io::Error::other("its pipes are missing"));
        };
        Ok(Self {
            process: Box::new(child),
            input: Box::new(input),
            output: Box::new(output),
            guard,
        })
    }
}
