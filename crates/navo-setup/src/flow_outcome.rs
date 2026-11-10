//! The wizard's outcome types: how a runner action ended and what a running-Navo scan found.

use crate::mode::Mode;
use crate::syntax::FolderProblem;

/// Verification, including marker handling, has completed before either success is reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completion {
    Installed,
    NavoFinishedUpdate,
}

impl Completion {
    /// Returns W1's additional success notice when Navo completed its own update.
    pub fn notice(self) -> Option<&'static str> {
        match self {
            Self::Installed => None,
            Self::NavoFinishedUpdate => Some("Navo finished the update itself."),
        }
    }
}

/// Runner failures distinguish safe retries from files or recovery sessions to keep.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    Folder(FolderProblem),
    EmptyRoot(Option<u32>),
    PartialRoot(Option<u32>),
    Upgrade(Option<u32>),
    Changed,
    Unmarked,
    Remarked,
    Unlisted,
    NavoStarted,
    AfterExit(Option<u32>),
    /// Setup could not unpack its own installer before the child ran; nothing was installed.
    Extract,
    /// The window's worker thread is gone; nothing more can run.
    WorkerLost,
}

impl Failure {
    /// Returns W1's recovery notice when a mid-upgrade launch could not be verified.
    pub fn notice(&self) -> Option<&'static str> {
        match self {
            Self::NavoStarted => {
                Some("Navo was started during Setup. Close Navo and run Setup again.")
            }
            _ => None,
        }
    }
}

/// A runner result is sent only after the child exits and verification finishes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetupExit {
    Installed(Completion),
    Failed(Failure),
    Blocked,
    ModeChanged(Box<Mode>),
    Elevated,
    NoPayload,
}

/// Unknown scan results fail closed, just like a visible running Navo (IW9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunningScan {
    Clear,
    Running,
    Failed,
}
