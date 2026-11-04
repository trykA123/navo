//! Probes need Win32; elsewhere `Launched` cannot exist and `start` refuses.

use std::convert::Infallible;
use std::path::Path;
use std::time::Duration;

use super::{Arg, CloseOutcome, CpuTime, Key, Memory, ProbeError, ThreadSwitches};
use crate::guard::ScratchRoot;

/// A running process inside its job. Uninhabited off Windows.
#[derive(Debug)]
pub struct Launched {
    never: Infallible,
}

impl Launched {
    /// Checks the arguments like Windows does, then refuses.
    pub fn start(_exe: &Path, root: &ScratchRoot, args: &[Arg<'_>]) -> Result<Self, ProbeError> {
        super::args::build(root, args)?;
        Err(ProbeError::Unsupported)
    }

    pub fn pid(&self) -> u32 {
        match self.never {}
    }

    pub fn wait_for_window(&mut self, _timeout: Duration) -> Result<Duration, ProbeError> {
        match self.never {}
    }

    pub fn memory(&self) -> Result<Memory, ProbeError> {
        match self.never {}
    }

    pub fn cpu_time(&self) -> Result<CpuTime, ProbeError> {
        match self.never {}
    }

    pub fn thread_switches(&self) -> Result<Vec<ThreadSwitches>, ProbeError> {
        match self.never {}
    }

    pub fn post_keys(&self, _keys: &[Key]) -> Result<(), ProbeError> {
        match self.never {}
    }

    pub fn wait_exit(&mut self, _timeout: Duration) -> Result<Option<i32>, ProbeError> {
        match self.never {}
    }

    pub fn close(self, _deadline: Duration) -> Result<CloseOutcome, ProbeError> {
        match self.never {}
    }
}
