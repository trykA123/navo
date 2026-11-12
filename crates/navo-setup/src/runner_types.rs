//! Opaque runner handles and small result types shared with the Host boundary.

use std::any::Any;
use std::fmt;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crate::mode::SqVersion;
use crate::progress_log::InstallStage;

/// A child process with a timed wait, owned by the runner until it exits.
pub trait Child {
    /// Waits up to `timeout` and returns the exit code once the child exits.
    fn wait_timeout(&mut self, timeout: Duration) -> io::Result<Option<i32>>;
    /// Waits without a deadline and returns the exit code.
    fn wait(&mut self) -> io::Result<i32>;
}

/// The host's held folders, kept opaque so the runner compiles and tests off Windows.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "the Windows host is the only consumer")
)]
pub struct Pin(Box<dyn Any>);

#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "the Windows host is the only consumer")
)]
impl Pin {
    /// Wraps one platform hold value.
    pub(crate) fn new<T: 'static>(state: T) -> Self {
        Self(Box::new(state))
    }

    /// Borrows the platform hold value, or `None` for another host's value.
    pub(crate) fn get<T: 'static>(&self) -> Option<&T> {
        self.0.downcast_ref()
    }

    /// Takes the platform hold value back for cleanup.
    pub(crate) fn take<T: 'static>(self) -> io::Result<T> {
        self.0
            .downcast::<T>()
            .map(|state| *state)
            .map_err(|_| io::Error::other("host pin type mismatch"))
    }
}

impl fmt::Debug for Pin {
    /// Writes an opaque name so sessions stay debuggable without exposing handles.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Pin(..)")
    }
}

/// The extracted Setup.exe and the private work folder that owns it.
pub struct Extracted {
    pub path: PathBuf,
    #[cfg_attr(
        not(windows),
        allow(dead_code, reason = "the Windows host is the only consumer")
    )]
    state: Box<dyn Any>,
}

#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "the Windows host is the only consumer")
)]
impl Extracted {
    /// Wraps the path used in IW3 and the platform cleanup state.
    pub(crate) fn new<T: 'static>(path: PathBuf, state: T) -> Self {
        Self {
            path,
            state: Box::new(state),
        }
    }

    /// Takes the platform cleanup state.
    pub(crate) fn take<T: 'static>(self) -> io::Result<T> {
        self.state
            .downcast::<T>()
            .map(|state| *state)
            .map_err(|_| io::Error::other("host extraction type mismatch"))
    }
}

impl fmt::Debug for Extracted {
    /// Shows the checked Setup.exe path.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Extracted")
            .field("path", &self.path)
            .finish()
    }
}

/// The marker's recorded file identity, for later checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkerId {
    pub volume: u64,
    pub id: [u8; 16],
}

/// How HKCU's uninstall entry compares with the held root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Registry {
    Absent,
    NamesRoot,
    NamesOther(PathBuf),
}

/// The two file checks IW4 reads after the child exits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verified {
    pub update_exe: bool,
    pub sq_version: Option<SqVersion>,
}

/// The folders a cleanup removed or kept.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cleanup {
    pub kept: Vec<PathBuf>,
}

/// One progress event: elapsed whole seconds, the 180 s flag and the verified work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub elapsed: Duration,
    pub still_working: bool,
    /// `None` whenever no percentage is verified, which is the UI's pulse.
    pub percent: Option<u8>,
    pub stage: Option<InstallStage>,
}

impl Progress {
    /// Timing only, for a wait that tails no log.
    pub fn from_elapsed(elapsed: Duration, still_working: bool) -> Self {
        Self {
            elapsed,
            still_working,
            percent: None,
            stage: None,
        }
    }
}
