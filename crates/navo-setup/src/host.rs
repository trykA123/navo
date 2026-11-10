//! Platform boundary for the install runner: probes, holds, payload and process scan.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use navo_core::FileIdentity;

use crate::{
    marker::Marker,
    mode::Discovery,
    payload::Payload,
    placement::Space,
    runner::{Child, Cleanup, Extracted, MarkerId, Pin, Registry, Verified},
    syntax::FolderProblem,
    upgrade::Staged,
};

#[cfg(windows)]
#[path = "host_windows.rs"]
mod windows;
#[cfg(windows)]
pub use windows::WindowsHost;

/// A folder refusal kept inside an `io::Error` so the runner can name the reason.
#[derive(Debug)]
pub struct FolderRefusal(pub FolderProblem);

impl fmt::Display for FolderRefusal {
    /// Writes the folder problem's user-facing line.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.message())
    }
}

impl std::error::Error for FolderRefusal {}

/// Wraps one folder problem as an I/O error without losing its reason.
pub fn folder_error(problem: FolderProblem) -> io::Error {
    io::Error::other(FolderRefusal(problem))
}

/// The paths removed and kept by the post-install shortcut pass.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShortcutReport {
    pub removed: Vec<PathBuf>,
    pub kept: Vec<PathBuf>,
}

/// The platform steps W8b and W8s call; every error fails closed.
pub trait Host {
    /// Whether Setup runs elevated; any error counts as elevated (IW13).
    fn elevated(&mut self) -> io::Result<bool>;
    /// Reads the registry and legacy installs for the mode decision.
    fn discover(&mut self) -> io::Result<Discovery>;
    /// Creates and holds every missing folder for a fresh root with its space need.
    fn pin_fresh(&mut self, root: &Path, space: &Space) -> io::Result<Pin>;
    /// Holds an existing root whose identity must equal discovery's.
    fn pin_existing(&mut self, root: &Path, expected: FileIdentity) -> io::Result<Pin>;
    /// Rechecks the existing ancestors, the held folders and the refused set.
    fn ancestors_unchanged(&mut self, pin: &Pin) -> io::Result<bool>;
    /// Proves the held upgrade root accepts a new file (IW8b step 2).
    fn writable(&mut self, pin: &Pin) -> io::Result<bool>;
    /// Whether a process runs Navo from the root; any scan error counts as running.
    fn running(&mut self, root: &Path) -> io::Result<bool>;
    /// Whether the held fresh root has no entries; errors count as not empty.
    fn root_empty(&mut self, pin: &Pin) -> io::Result<bool>;
    /// Extracts Setup.exe to the private work folder and holds it until the child exits.
    fn extract(&mut self, payload: &Payload) -> io::Result<Extracted>;
    /// Writes the nupkg into `packages\` and returns its staged path.
    fn stage_package(&mut self, pin: &Pin, payload: &Payload) -> io::Result<Staged>;
    /// Replaces `Update.exe` with the nupkg's Squirrel.exe and returns its SHA-256.
    fn replace_update_exe(&mut self, pin: &Pin, payload: &Payload) -> io::Result<[u8; 32]>;
    /// Whether HKCU names the held root's identity (and, when fresh, its exact text).
    fn registry_root(&mut self, pin: &Pin) -> io::Result<Registry>;
    /// Starts the one Velopack process with an argument array and no shell.
    fn spawn(&mut self, args: &[OsString]) -> io::Result<Box<dyn Child>>;
    /// Reads `Update.exe` (and its hash when expected) and `current\sq.version`.
    fn verify_files(
        &mut self,
        root: &Path,
        update_sha256: Option<[u8; 32]>,
    ) -> io::Result<Verified>;
    /// Writes the marker with `CREATE_NEW` through the held root.
    fn write_marker(&mut self, pin: &Pin, marker: &Marker) -> io::Result<MarkerId>;
    /// Reads a marker file; absent is `None`, a damaged file is an error.
    fn read_marker(&mut self, root: &Path) -> io::Result<Option<Marker>>;
    /// Removes only the exact unticked links after a successful fresh install.
    fn remove_shortcuts(&mut self, root: &Path, desktop: bool, start_menu: bool) -> ShortcutReport;
    /// Closes the session; `remove_created` runs IW7a's empty-folder cleanup.
    fn cleanup(
        &mut self,
        pin: Pin,
        extracted: Option<Extracted>,
        remove_created: bool,
    ) -> io::Result<Cleanup>;
}

/// One directory entry's shape for the uninstall-leftover rule.
struct Leftover {
    name: OsString,
    folder: bool,
    reparse: bool,
    empty: bool,
}

/// Whether the entries are a regular `Update.exe` plus empty, non-reparse folders.
fn leftovers_shape(entries: &[Leftover]) -> bool {
    let mut update = false;
    for entry in entries {
        if entry.reparse {
            return false;
        }
        if entry.folder {
            if !entry.empty {
                return false;
            }
        } else if entry.name.eq_ignore_ascii_case(OsStr::new("Update.exe")) {
            update = true;
        } else {
            return false;
        }
    }
    update
}

/// Whether `root` holds only a Velopack uninstall leftover: a regular `Update.exe` and empty folders.
///
/// Velopack's uninstaller can leave `Update.exe` and empty folders behind. Setup accepts
/// exactly that shape as a fresh root; any other file, non-empty folder or reparse point refuses it.
pub(crate) fn uninstall_leftovers_only(root: &Path) -> io::Result<bool> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let metadata = std::fs::symlink_metadata(entry.path())?;
        let folder = metadata.is_dir();
        let reparse = is_reparse(&metadata);
        let empty = folder && !reparse && std::fs::read_dir(entry.path())?.next().is_none();
        entries.push(Leftover {
            name: entry.file_name(),
            folder,
            reparse,
            empty,
        });
    }
    Ok(leftovers_shape(&entries))
}

/// Whether the metadata carries a reparse point: a Windows attribute or a symlink elsewhere.
#[cfg(windows)]
fn is_reparse(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one fabricated entry for the pure rule.
    fn leftover(name: &str, folder: bool, reparse: bool, empty: bool) -> Leftover {
        Leftover {
            name: name.into(),
            folder,
            reparse,
            empty,
        }
    }

    #[test]
    fn the_rule_requires_a_regular_update_exe() {
        assert!(leftovers_shape(&[
            leftover("Update.exe", false, false, true),
            leftover("empty", true, false, true),
        ]));
        assert!(!leftovers_shape(&[leftover("empty", true, false, true)]));
        assert!(!leftovers_shape(&[leftover(
            "Update.exe",
            true,
            false,
            true
        )]));
        assert!(!leftovers_shape(&[
            leftover("Update.exe", false, false, true),
            leftover("other.txt", false, false, true),
        ]));
        assert!(!leftovers_shape(&[
            leftover("Update.exe", false, false, true),
            leftover("full", true, false, false),
        ]));
        assert!(!leftovers_shape(&[leftover(
            "Update.exe",
            false,
            true,
            true
        )]));
        assert!(!leftovers_shape(&[
            leftover("Update.exe", false, false, true),
            leftover("link", true, true, false),
        ]));
    }

    /// Builds a temp folder with the given files and empty folders.
    fn folder(files: &[&str], dirs: &[&str]) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        for file in files {
            std::fs::write(root.path().join(file), b"x").unwrap();
        }
        for dir in dirs {
            std::fs::create_dir(root.path().join(dir)).unwrap();
        }
        root
    }

    #[test]
    fn an_update_leftover_with_empty_folders_is_accepted() {
        let root = folder(&["Update.exe"], &["empty", "also-empty"]);
        assert!(uninstall_leftovers_only(root.path()).unwrap());
        let lower = folder(&["update.EXE"], &[]);
        assert!(uninstall_leftovers_only(lower.path()).unwrap());
    }

    #[test]
    fn empty_folders_without_an_update_leftover_are_refused() {
        let root = folder(&[], &["src", "docs"]);
        assert!(!uninstall_leftovers_only(root.path()).unwrap());
    }

    #[test]
    fn anything_else_is_refused() {
        let other = folder(&["Update.exe", "other.txt"], &[]);
        assert!(!uninstall_leftovers_only(other.path()).unwrap());
        let setup = folder(&["Setup.exe"], &[]);
        assert!(!uninstall_leftovers_only(setup.path()).unwrap());
        let full = folder(&["Update.exe"], &["full"]);
        std::fs::write(full.path().join("full/child"), b"x").unwrap();
        assert!(!uninstall_leftovers_only(full.path()).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn a_reparse_entry_is_refused() {
        use std::os::unix::fs::symlink;
        let root = folder(&["Update.exe"], &[]);
        symlink(root.path().join("Update.exe"), root.path().join("link")).unwrap();
        assert!(!uninstall_leftovers_only(root.path()).unwrap());
    }
}
