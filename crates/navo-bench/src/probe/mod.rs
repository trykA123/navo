//! Probes on a running Navo: start in a kill-on-close job, main window by PID, memory, CPU
//! time, per-thread context switches, keys posted to that window only, close then terminate.
//!
//! Wakeups come from `NtQuerySystemInformation(SystemProcessInformation)`: its documented
//! `SYSTEM_THREAD_INFORMATION.Reserved3` is the thread's context-switch count, readable without
//! admin. Documented alternatives (`QueryThreadCycleTime`, ETW) give cycles or need admin.

mod args;
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "used by the Win32 probes, tested everywhere")
)]
mod keys;
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "used by the Win32 probes, tested everywhere")
)]
mod math;
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "used by the Win32 probes, tested everywhere")
)]
mod sysinfo;

#[cfg(not(windows))]
#[path = "portable.rs"]
mod platform;
#[cfg(windows)]
#[path = "windows.rs"]
mod platform;
#[cfg(windows)]
#[path = "spawn.rs"]
mod spawn;
#[cfg(windows)]
#[path = "win32.rs"]
#[allow(
    unsafe_code,
    reason = "job objects, window lookup, process counters and posted messages are Win32 calls"
)]
mod win32;

pub(crate) use args::verify_outputs;
pub use args::{Arg, PathFlag};
pub use keys::{Key, Letter};
pub use math::{CpuTime, ThreadDelta, ThreadSwitches, per_second, switch_deltas};
pub use platform::Launched;

use crate::guard::GuardError;

/// Process memory from `GetProcessMemoryInfo`, in bytes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Memory {
    pub private_bytes: u64,
    pub working_set_bytes: u64,
}

/// How a launched process ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CloseOutcome {
    /// False when the deadline passed and the process was terminated.
    pub polite: bool,
    pub exit_code: Option<i32>,
}

/// The Win32 step that failed; carried by `ProbeError::Os` with the OS error code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Step {
    CreateJob,
    LimitJob,
    AssignJob,
    Resume,
    Memory,
    Times,
    Switches,
    Post,
    Wait,
    Terminate,
}

/// Why a probe failed. Holds roles and codes only, never a path or a name.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProbeError {
    #[error("probe:windows-only")]
    Unsupported,
    #[error("probe:argument-{0}")]
    Guard(GuardError),
    /// A path argument was not a plain relative path inside the run root.
    #[error("probe:argument-not-relative")]
    NotRelative,
    /// A read-only folder argument that was relative, inside the run root, or on a write flag.
    #[error("probe:argument-not-read-only")]
    BadReadOnly,
    /// A file the child was told to write is not a regular file inside the run root.
    #[error("probe:output-missing")]
    OutputMissing,
    #[error("probe:switch-invalid")]
    BadSwitch,
    #[error("probe:spawn-{0:?}")]
    Spawn(std::io::ErrorKind),
    #[error("probe:{step:?}-os-{code}")]
    Os { step: Step, code: u32 },
    #[error("probe:no-window")]
    NoWindow,
    #[error("probe:exited")]
    Exited,
    /// The target window now belongs to another process; nothing was posted.
    #[error("probe:wrong-process")]
    WrongProcess,
    #[error("probe:sysinfo-malformed")]
    Malformed,
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod args_tests;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
#[cfg(test)]
#[cfg(windows)]
#[path = "windows_tests.rs"]
mod windows_tests;
