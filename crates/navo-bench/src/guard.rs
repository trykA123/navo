//! The only module allowed to change the filesystem; every write path goes through it.

mod act;
#[cfg(not(windows))]
#[path = "guard/act_portable.rs"]
mod act_platform;
#[cfg(windows)]
#[path = "guard/act_windows.rs"]
#[allow(
    unsafe_code,
    reason = "handle-verified acts and the Recycle Bin require Win32 and COM calls"
)]
mod act_platform;
mod error;
mod ledger;
mod placement;
#[cfg(not(windows))]
#[path = "guard/portable.rs"]
mod platform;
#[cfg(windows)]
#[path = "guard/windows.rs"]
#[allow(
    unsafe_code,
    reason = "handle-based Windows path validation requires Win32 calls"
)]
mod platform;
mod resolve;
#[cfg(windows)]
#[path = "guard/shell_sink.rs"]
#[allow(
    unsafe_code,
    reason = "the Recycle Bin move and its progress sink are COM calls"
)]
mod shell_sink;
mod stale;
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "used by the Windows recycler, tested everywhere")
)]
mod veto;
mod write;

pub use act::{Recycler, VerifiedItem};
#[cfg(windows)]
pub use act_platform::ShellRecycler;
pub use error::{GuardError, PathRole, RecycleError};
pub(crate) use placement::forbidden_roots;
pub use stale::StaleCleanup;
pub use write::WRITE_API;

use std::fmt;
use std::path::Path;

use resolve::Resolved;

pub(crate) const MARKER_FILE: &str = ".navo-bench-root";

/// Owns a fresh, marked run directory. Dropping it closes handles without deleting data.
pub struct ScratchRoot {
    guard: Guard,
    token: [u8; 32],
    pin: std::fs::File,
}

/// A new folder for the finished report, beside the run root; files in it are only made new.
pub struct OutputFolder {
    guard: Guard,
    pin: std::fs::File,
}

/// Validates destinations against one run directory and retains its directory handles.
pub struct Guard {
    root: Resolved,
}

/// A validated destination tied to its guard; construction and filesystem access stay private.
pub struct CheckedPath<'a> {
    resolved: Resolved,
    _guard: &'a Guard,
}

impl ScratchRoot {
    /// Requires an existing parent and refuses any overlap with the owner's chosen folder.
    /// Lists the run's token in the parent's ledger before the folder is made.
    pub fn create(parent: &Path, owner: Option<&Path>) -> Result<Self, GuardError> {
        placement::create(parent, owner)
    }

    pub fn path(&self) -> &Path {
        &self.guard.root.path
    }

    pub fn token(&self) -> &[u8; 32] {
        &self.token
    }

    pub fn guard(&self) -> &Guard {
        &self.guard
    }

    pub fn check(&self, path: &Path) -> Result<CheckedPath<'_>, GuardError> {
        self.guard.check(path)
    }
}

impl OutputFolder {
    /// Makes the folder at `path`, which must not exist yet. Its parent passes the same rules as
    /// a run root's parent, and the folder never overlaps the owner's chosen folder.
    pub fn create(path: &Path, owner: Option<&Path>) -> Result<Self, GuardError> {
        placement::create_output(path, owner)
    }

    pub fn path(&self) -> &Path {
        &self.guard.root.path
    }
}

impl Guard {
    /// Rejects ambiguous names, escapes and every symlink or reparse point on the path.
    pub fn check(&self, path: &Path) -> Result<CheckedPath<'_>, GuardError> {
        let resolved = resolve::resolve(path, PathRole::ScratchRoot)?;
        if !resolve::contains(&self.root.path, &resolved.path) {
            return Err(GuardError::OutsideRoot(PathRole::ScratchRoot));
        }
        #[cfg(windows)]
        platform::verify_root(&self.root, &resolved)?;
        Ok(CheckedPath {
            resolved,
            _guard: self,
        })
    }
}

impl fmt::Debug for ScratchRoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ScratchRoot { scratch-root }")
    }
}

impl fmt::Debug for OutputFolder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OutputFolder { output-folder }")
    }
}

impl fmt::Debug for Guard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Guard { scratch-root }")
    }
}

impl fmt::Debug for CheckedPath<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CheckedPath { scratch-root }")
    }
}

#[cfg(test)]
#[path = "fixture.rs"]
pub(crate) mod fixture;
#[cfg(test)]
#[path = "guard/tests.rs"]
mod tests;
#[cfg(test)]
#[cfg(unix)]
#[path = "guard/unix_tests.rs"]
mod unix_tests;
#[cfg(test)]
#[cfg(windows)]
#[path = "guard/windows_tests.rs"]
#[allow(unsafe_code, reason = "Windows alias tests require Win32 calls")]
mod windows_tests;

#[cfg(test)]
#[path = "guard/test_support.rs"]
pub(crate) mod test_support;

#[cfg(test)]
#[path = "guard/write_tests.rs"]
mod write_tests;

#[cfg(test)]
#[path = "guard/output_tests.rs"]
mod output_tests;

#[cfg(test)]
#[cfg(unix)]
#[path = "guard/recycle_tests.rs"]
mod recycle_tests;

#[cfg(test)]
#[cfg(unix)]
#[path = "guard/cleanup_tests.rs"]
mod cleanup_tests;

#[cfg(test)]
#[cfg(unix)]
#[path = "guard/stray_tests.rs"]
mod stray_tests;

#[cfg(test)]
#[cfg(unix)]
#[path = "guard/ledger_tests.rs"]
mod ledger_tests;

#[cfg(test)]
#[cfg(windows)]
#[path = "guard/act_windows_tests.rs"]
mod act_windows_tests;
