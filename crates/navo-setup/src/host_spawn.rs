//! Windows child spawn for the Host: starts the Velopack child and polls it to its exit.

use std::ffi::OsString;
use std::io;
use std::process::{Child as ProcessChild, Command, Stdio};
use std::time::{Duration, Instant};

use crate::runner::Child;

/// Spawns one child with no shell and detached standard streams.
pub(super) fn spawn_detached(args: &[OsString]) -> io::Result<Box<dyn Child>> {
    let (program, rest) = args
        .split_first()
        .ok_or_else(|| io::Error::other("empty Velopack argument array"))?;
    let child = Command::new(program)
        .args(rest)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(Box::new(Process(child)))
}

/// The one Velopack child, polled at 25 ms so IW4's progress events stay close to seconds.
struct Process(ProcessChild);

impl Child for Process {
    /// Polls until the deadline; `None` means the child still runs.
    fn wait_timeout(&mut self, timeout: Duration) -> io::Result<Option<i32>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.0.try_wait()? {
                return Ok(Some(status.code().unwrap_or(-1)));
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            std::thread::sleep(
                deadline
                    .saturating_duration_since(now)
                    .min(Duration::from_millis(25)),
            );
        }
    }

    /// Waits without a deadline and returns the exit code.
    fn wait(&mut self) -> io::Result<i32> {
        Ok(self.0.wait()?.code().unwrap_or(-1))
    }
}
