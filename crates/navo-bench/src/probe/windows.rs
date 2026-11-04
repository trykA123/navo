use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::path::Path;
use std::process::Child;
use std::time::{Duration, Instant};

use super::keys::{self, Message, Target, WM_CLOSE, WindowId};
use super::math::poll;
use super::win32::{self, Win32Windows};
use super::{Arg, CloseOutcome, CpuTime, Key, Memory, ProbeError, Step, ThreadSwitches, spawn};
use crate::guard::ScratchRoot;

const POLL: Duration = Duration::from_millis(2);
const EXIT_POLL: Duration = Duration::from_millis(50);

/// A running process inside its own kill-on-close job. Dropping it ends the process tree.
pub struct Launched {
    pub(super) child: Child,
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "held only so that closing it kills the tree")
    )]
    pub(super) job: OwnedHandle,
    started: Instant,
    pub(super) window: Option<WindowId>,
}

impl Launched {
    /// Starts `exe` suspended, puts it in a new job, then lets it run. The clock starts
    /// just before process creation.
    pub fn start(exe: &Path, root: &ScratchRoot, args: &[Arg<'_>]) -> Result<Self, ProbeError> {
        let job = win32::kill_on_close_job()?;
        // Built after the job exists, so the guard checks run right before the process starts.
        let argv = super::args::build(root, args)?;
        let started = Instant::now();
        let mut child =
            spawn::suspended(exe, &argv).map_err(|error| ProbeError::Spawn(error.kind()))?;
        let entered = win32::assign(&job, child.as_raw_handle())
            .and_then(|()| win32::resume_only_thread(child.id()));
        if let Err(error) = entered {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        Ok(Self {
            child,
            job,
            started,
            window: None,
        })
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Waits for the first visible, unowned top-level window of this process and returns
    /// the time since the start, within one poll interval (2 ms).
    pub fn wait_for_window(&mut self, timeout: Duration) -> Result<Duration, ProbeError> {
        let pid = self.child.id();
        let child = &mut self.child;
        let found = poll(timeout, POLL, || {
            if let Some(window) = keys::main_window(&win32::top_windows(), pid) {
                return Ok(Some(window));
            }
            match child.try_wait() {
                Ok(None) => Ok(None),
                Ok(Some(_)) => Err(ProbeError::Exited),
                Err(error) => Err(ProbeError::Spawn(error.kind())),
            }
        })?;
        let elapsed = self.started.elapsed();
        let window = found.ok_or(ProbeError::NoWindow)?;
        self.window = Some(window);
        Ok(elapsed)
    }

    pub fn memory(&self) -> Result<Memory, ProbeError> {
        win32::memory(self.child.as_raw_handle())
    }

    pub fn cpu_time(&self) -> Result<CpuTime, ProbeError> {
        win32::cpu_time(self.child.as_raw_handle())
    }

    /// Context-switch counters of this process's threads (not of its child processes).
    pub fn thread_switches(&self) -> Result<Vec<ThreadSwitches>, ProbeError> {
        win32::thread_switches(self.child.id())
    }

    /// Posts each key to this process's main window only; stops at the first refusal.
    pub fn post_keys(&self, keys: &[Key]) -> Result<(), ProbeError> {
        keys::post_keys(&Win32Windows, self.target()?, keys)
    }

    /// Waits up to `timeout` for the process to end by itself; `None` when it still runs.
    pub fn wait_exit(&mut self, timeout: Duration) -> Result<Option<i32>, ProbeError> {
        let child = &mut self.child;
        let status = poll(timeout, EXIT_POLL, || {
            child.try_wait().map_err(|_| os(Step::Wait))
        })?;
        Ok(status.map(|status| status.code().unwrap_or(-1)))
    }

    /// Posts `WM_CLOSE` to the main window, waits up to `deadline`, then terminates.
    pub fn close(mut self, deadline: Duration) -> Result<CloseOutcome, ProbeError> {
        let asked = self.close_target().and_then(|target| {
            let message = Message {
                id: WM_CLOSE,
                wparam: 0,
                lparam: 0,
            };
            keys::post_verified(&Win32Windows, target, message)
        });
        let wait = if asked.is_ok() {
            deadline
        } else {
            Duration::ZERO
        };
        let child = &mut self.child;
        let exited = poll(wait, POLL, || child.try_wait().map_err(|_| os(Step::Wait)))?;
        if let Some(status) = exited {
            return Ok(CloseOutcome {
                polite: true,
                exit_code: status.code(),
            });
        }
        if self.child.kill().is_err() {
            // It may have exited on its own right after the deadline.
            let status = self.child.try_wait().map_err(|_| os(Step::Wait))?;
            return status
                .map(|status| CloseOutcome {
                    polite: true,
                    exit_code: status.code(),
                })
                .ok_or_else(|| os(Step::Terminate));
        }
        let status = self.child.wait().map_err(|_| os(Step::Wait))?;
        Ok(CloseOutcome {
            polite: false,
            exit_code: status.code(),
        })
    }

    fn close_target(&self) -> Result<Target, ProbeError> {
        let pid = self.child.id();
        let window = self
            .window
            .or_else(|| keys::main_window(&win32::top_windows(), pid))
            .ok_or(ProbeError::NoWindow)?;
        Ok(Target { window, pid })
    }

    fn target(&self) -> Result<Target, ProbeError> {
        let window = self.window.ok_or(ProbeError::NoWindow)?;
        Ok(Target {
            window,
            pid: self.child.id(),
        })
    }
}

fn os(step: Step) -> ProbeError {
    ProbeError::Os { step, code: 0 }
}

impl Drop for Launched {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        // `job` closes after this and kills any process the child started.
    }
}

impl std::fmt::Debug for Launched {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Launched")
            .field("pid", &self.child.id())
            .finish_non_exhaustive()
    }
}
