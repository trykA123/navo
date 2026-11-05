use std::io;
use std::time::Duration;

pub(crate) use crate::REMOVE_DEADLINE;
pub(crate) const STOP_LIMIT: Duration = Duration::from_secs(12);
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
    /// Waits on the kept handle; `false` when the process still runs after `limit`.
    fn wait(&mut self, limit: Duration) -> io::Result<bool>;
    fn elapsed(&self) -> Duration;
    fn pause(&mut self);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StopLimits {
    pub stop: Duration,
    pub wait: Duration,
}

impl StopLimits {
    pub(crate) const FULL: Self = Self {
        stop: STOP_LIMIT,
        wait: PROCESS_WAIT,
    };

    pub(crate) fn within(remaining: Duration) -> Self {
        let stop = STOP_LIMIT.min(remaining);
        Self {
            stop,
            wait: PROCESS_WAIT.min(remaining.saturating_sub(stop)),
        }
    }
}

/// Stops the service and waits for its process to end; a timeout leaves it registered.
pub(crate) fn stop(control: &mut impl Control, limits: StopLimits) -> io::Result<()> {
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
        if control.elapsed() >= limits.stop {
            return Err(timed_out("the index service did not stop in time"));
        }
        control.pause();
    }
    if held.process.is_some() && !control.wait(limits.wait)? {
        return Err(timed_out("the index service process did not exit in time"));
    }
    Ok(())
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
