//! Service stop and start steps over a small SCM control trait, so tests drive fake states.

use std::io;
use std::time::{Duration, Instant};

pub(crate) use crate::REMOVE_DEADLINE;
/// Remove's stop limit; the 20 s remove deadline still shares the file cleanup.
pub(crate) const STOP_LIMIT: Duration = Duration::from_secs(12);
/// Install's stop and start limit: it covers the service's 30 s stop hint and the process wait.
pub(crate) const INSTALL_STOP_LIMIT: Duration = Duration::from_secs(35);
pub(crate) const PROCESS_WAIT: Duration = Duration::from_secs(5);
pub(crate) const POLL: Duration = Duration::from_millis(100);

/// The SCM `dwCurrentState` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    Stopped,
    StartPending,
    StopPending,
    Running,
    ContinuePending,
    PausePending,
    Paused,
}

impl State {
    pub(crate) fn from_raw(raw: u32) -> io::Result<Self> {
        Ok(match raw {
            1 => Self::Stopped,
            2 => Self::StartPending,
            3 => Self::StopPending,
            4 => Self::Running,
            5 => Self::ContinuePending,
            6 => Self::PausePending,
            7 => Self::Paused,
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unknown service state {other}"),
                ));
            }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Status {
    pub state: State,
    pub process: u32,
}

/// The SCM and process calls `stop` makes; Windows implements it, tests fake it.
pub(crate) trait Control {
    fn status(&mut self) -> io::Result<Status>;
    /// Opens and keeps a handle to `process` for the wait.
    fn open(&mut self, process: u32) -> io::Result<()>;
    fn release(&mut self);
    /// Sends the stop control; `false` when the service cannot accept it yet.
    fn stop(&mut self) -> io::Result<bool>;
    /// Sends the start control; reports an SCM already-running answer without failing.
    fn start(&mut self) -> io::Result<StartResult>;
    /// Waits on the kept handle; `false` when the process still runs after `limit`.
    fn wait(&mut self, limit: Duration) -> io::Result<bool>;
    fn now(&self) -> Instant;
    fn pause(&mut self);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartResult {
    Started,
    AlreadyRunning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StopLimits {
    pub stop: Duration,
    pub deadline: Option<Instant>,
}

impl StopLimits {
    #[cfg(test)]
    pub(crate) const FULL: Self = Self {
        stop: STOP_LIMIT,
        deadline: None,
    };

    /// Install steps get the longer limit that the service's 30 s wait hint needs.
    pub(crate) const INSTALL: Self = Self {
        stop: INSTALL_STOP_LIMIT,
        deadline: None,
    };

    pub(crate) fn within(deadline: Instant) -> Self {
        Self {
            stop: STOP_LIMIT,
            deadline: Some(deadline),
        }
    }
}

/// Stops the service and waits for its process to end; a timeout leaves it registered.
pub(crate) fn stop(control: &mut impl Control, limits: StopLimits) -> io::Result<()> {
    let stop_deadline = limits
        .deadline
        .map_or(control.now() + limits.stop, |deadline| {
            deadline.min(control.now() + limits.stop)
        });
    let mut held = Held::default();
    loop {
        let status = control.status()?;
        match status.state {
            State::Stopped => break,
            State::StartPending => {}
            State::StopPending => {
                if status.process != 0 && held.process.is_none() {
                    held.process = retain(control, status.process)?;
                }
            }
            State::Running | State::Paused | State::PausePending | State::ContinuePending => {
                held.stop(control, status.process)?;
            }
        }
        if control.now() >= stop_deadline {
            return Err(timed_out("the index service did not stop in time"));
        }
        control.pause();
    }
    let wait = limits.deadline.map_or(PROCESS_WAIT, |deadline| {
        PROCESS_WAIT.min(deadline.saturating_duration_since(control.now()))
    });
    if held.process.is_some() && !control.wait(wait)? {
        return Err(timed_out("the index service process did not exit in time"));
    }
    Ok(())
}

/// Waits out a stop in progress, then starts the service and polls until it runs.
/// A `STOP_PENDING` service that outlasts the install limit is reported as a timeout.
pub(crate) fn start(control: &mut impl Control, limits: StopLimits) -> io::Result<()> {
    let deadline = limits
        .deadline
        .map_or(control.now() + limits.stop, |deadline| {
            deadline.min(control.now() + limits.stop)
        });
    loop {
        let status = control.status()?;
        match status.state {
            State::Stopped => break,
            State::StopPending => {
                if control.now() >= deadline {
                    return Err(still_stopping(&status, limits.stop));
                }
                control.pause();
            }
            State::Running | State::StartPending => {
                return wait_running(control, deadline, limits.stop);
            }
            State::Paused | State::PausePending | State::ContinuePending => {
                return wait_running(control, deadline, limits.stop);
            }
        }
    }
    loop {
        match control.start()? {
            StartResult::Started => return wait_running(control, deadline, limits.stop),
            StartResult::AlreadyRunning => match control.status()?.state {
                State::Stopped => {}
                State::StopPending => wait_stopped(control, deadline, limits.stop)?,
                State::Running | State::StartPending => {
                    return wait_running(control, deadline, limits.stop);
                }
                State::Paused | State::PausePending | State::ContinuePending => {
                    return wait_running(control, deadline, limits.stop);
                }
            },
        }
    }
}

/// Polls until `STOPPED`; a `STOP_PENDING` service at the limit is named in the timeout.
fn wait_stopped(control: &mut impl Control, deadline: Instant, limit: Duration) -> io::Result<()> {
    loop {
        let status = control.status()?;
        if status.state == State::Stopped {
            return Ok(());
        }
        if control.now() >= deadline {
            return Err(if status.state == State::StopPending {
                still_stopping(&status, limit)
            } else {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "the index service did not reach STOPPED within {} s (state {:?})",
                        limit.as_secs(),
                        status.state
                    ),
                )
            });
        }
        control.pause();
    }
}

/// Polls until `RUNNING`; a stop or start that outlasts the limit is a timeout.
fn wait_running(control: &mut impl Control, deadline: Instant, limit: Duration) -> io::Result<()> {
    loop {
        let status = control.status()?;
        match status.state {
            State::Running => return Ok(()),
            State::Stopped => {
                return Err(io::Error::other(
                    "the index service stopped again after it was started",
                ));
            }
            State::StopPending => {
                if control.now() >= deadline {
                    return Err(still_stopping(&status, limit));
                }
            }
            _ if control.now() >= deadline => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "the index service did not reach RUNNING within {} s (state {:?})",
                        limit.as_secs(),
                        status.state
                    ),
                ));
            }
            _ => {}
        }
        control.pause();
    }
}

/// Reports the state and limit when a stop keeps `Start` waiting.
fn still_stopping(status: &Status, limit: Duration) -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        format!(
            "the index service is still STOP_PENDING after {} s (state {:?}); it was not started",
            limit.as_secs(),
            status.state
        ),
    )
}

/// The process whose handle `Control` keeps, and whether it accepted the stop.
#[derive(Default)]
struct Held {
    process: Option<u32>,
    sent: bool,
}

impl Held {
    fn stop(&mut self, control: &mut impl Control, process: u32) -> io::Result<()> {
        if self.process != Some(process) {
            if self.process.take().is_some() {
                control.release();
            }
            self.sent = false;
            self.process = retain(control, process)?;
        }
        if self.process.is_some() && !self.sent {
            self.sent = control.stop()?;
        }
        Ok(())
    }
}

/// Keeps a handle only while the SCM still reports the same live process.
fn retain(control: &mut impl Control, process: u32) -> io::Result<Option<u32>> {
    if process == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "a running service reported no process",
        ));
    }
    let opened = control.open(process);
    let now = control.status()?;
    if now.process == process && now.state != State::Stopped {
        opened?;
        return Ok(Some(process));
    }
    if opened.is_ok() {
        control.release();
    }
    Ok(None)
}

fn timed_out(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, message)
}

#[cfg(test)]
#[path = "stop_tests.rs"]
mod tests;
