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

/// Starts a helper from a spec. Contract: a hook that returns `Err` has no live child left.
pub type Spawn = Arc<dyn Fn(&HelperSpec) -> io::Result<Spawned> + Send + Sync>;

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
