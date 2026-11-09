//! Install runner: IW8b's eleven-step order over the platform Host boundary.

#![allow(
    clippy::result_large_err,
    reason = "the recovery variants must carry the open Session by design"
)]

use std::fmt;
use std::path::{Path, PathBuf};

use crate::flow::{Completion, Failure, Shortcuts};
use crate::marker::Marker;
use crate::mode::Mode;

#[path = "runner_types.rs"]
mod runner_types;
#[path = "runner_steps.rs"]
mod steps;
pub use runner_types::{Child, Cleanup, Extracted, MarkerId, Pin, Progress, Registry, Verified};

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;

/// A verified install; the completion adds W1's "Navo finished the update itself".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Installed {
    pub completion: Completion,
    pub legacy_marker_failed: bool,
}

/// A session keeps the held folders and extracted payload alive on recovery pages.
pub struct Session {
    pin: Pin,
    extracted: Option<Extracted>,
    marker: Option<Marker>,
    marker_id: Option<MarkerId>,
    root: PathBuf,
    upgrade: bool,
    shortcuts: Shortcuts,
    discovery_marker: Option<Marker>,
}

impl Session {
    /// The root this session holds.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The marker's file id once it was written.
    pub fn marker_id(&self) -> Option<MarkerId> {
        self.marker_id
    }

    /// Whether this session runs an upgrade rather than a fresh install.
    pub fn is_upgrade(&self) -> bool {
        self.upgrade
    }

    /// W0 Q4's uninstall command naming this root, for the Unlisted recovery text.
    pub fn uninstall_command(&self) -> String {
        format!("\"{}\\Update.exe\" uninstall", self.root.display())
    }
}

impl fmt::Debug for Session {
    /// Shows the root and recovery state without exposing platform handles.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Session")
            .field("root", &self.root)
            .field("upgrade", &self.upgrade)
            .field("shortcuts", &self.shortcuts)
            .field("discovery_marker", &self.discovery_marker)
            .field("marker", &self.marker)
            .field("marker_id", &self.marker_id)
            .finish_non_exhaustive()
    }
}

/// Every install outcome the shell or silent mode must act on.
#[derive(Debug)]
pub enum InstallError {
    Elevated,
    Blocked,
    AlreadyInstalled(PathBuf),
    ModeChanged(Box<Mode>),
    NoPayload,
    Extract,
    WrongAction,
    Failed(Failure),
    RetryEmpty(Session, Option<u32>),
    Unmarked(Session),
    Remarked(Session),
    Unlisted(Session),
}

impl InstallError {
    /// Returns IW17's silent exit code for this outcome.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Elevated => 7,
            Self::Blocked => 4,
            Self::AlreadyInstalled(_) => 3,
            Self::ModeChanged(found) => match found.as_ref() {
                Mode::Same(_) | Mode::Newer(_) => 8,
                Mode::Ambiguous(_) => 6,
                Mode::Fresh | Mode::Upgrade(_) => 3,
            },
            Self::NoPayload => 9,
            Self::Extract | Self::WrongAction => 5,
            Self::Failed(failure) => failure_code(failure),
            Self::RetryEmpty(_, _) | Self::Unmarked(_) | Self::Remarked(_) | Self::Unlisted(_) => 5,
        }
    }
}

/// Maps a runner failure to IW17's code.
fn failure_code(failure: &Failure) -> u8 {
    match failure {
        Failure::Folder(_) | Failure::Changed => 3,
        Failure::NavoStarted => 4,
        Failure::EmptyRoot(_)
        | Failure::PartialRoot(_)
        | Failure::Upgrade(_)
        | Failure::Unmarked
        | Failure::Remarked
        | Failure::Unlisted
        | Failure::AfterExit(_) => 5,
    }
}

#[path = "runner_run.rs"]
mod run;
pub use run::{close, install, mark_again, retry};
