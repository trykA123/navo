//! A local folder as a compare side through plain `std::fs`, for hosts without Navo's `LocalFs`.
//!
//! `StdSide` gives the same entries, stamps and errors as `LocalSide`; the parity test in
//! `tests/std_side.rs` holds the two together. Listing and stamps go through `navo_fs::walk`.

use std::fs::File;
use std::io::{self, Read};
use std::ops::ControlFlow;
use std::sync::atomic::AtomicBool;

use navo_compare::model::{
    CancelToken, EntryKind, ObjectId, ReadHint, Reason, RelPath, SideEntry, SideSource, Stamp,
};
use navo_core::{
    EntryKind as FsKind, FileIdentity, FsErrorKind, Location, ProviderId, ProviderPath,
};
use navo_fs::walk::{self, WalkEntry};
use navo_fs::{DriveClass, drive_class, needs_network_bound, os_path};

use super::entry::{child_rel, reason_of, segments};

/// A local folder read with `std::fs`. Names are kept as `LocalFs` keeps them: a name that is
/// not valid text is listed with its escaped spelling, which `location` turns back into bytes.
pub(crate) struct StdSide {
    root: Location,
    provider: ProviderId,
    volume: u64,
    network: bool,
}

impl StdSide {
    /// Opens a side on `root`; fails with a reason when it is missing or not a folder.
    pub(crate) fn new(root: Location) -> Result<Self, Reason> {
        let path = os_path(&root).map_err(|error| reason_of(error.kind))?;
        let found = walk::stat(&path).map_err(|error| fs_reason(&error))?;
        if found.kind != FsKind::Folder && !found.link_to_folder {
            return Err(Reason::Unsupported);
        }
        let volume = navo_fs::native_identity(&path)
            .ok()
            .flatten()
            .map_or(0, |identity| identity.volume);
        let network = is_network_class(drive_class(&root));
        Ok(Self {
            root,
            provider: ProviderId::local(),
            volume,
            network,
        })
    }

    /// The location of a row path below the root; fails when a segment is not a valid name.
    fn location(&self, rel: &RelPath) -> Result<Location, Reason> {
        let mut location = self.root.clone();
        for part in segments(rel) {
            location = location
                .join(&ProviderPath::from_text(&part))
                .map_err(|_| Reason::Unsupported)?;
        }
        Ok(location)
    }

    /// The native path of a row path.
    fn path(&self, rel: &RelPath) -> Result<std::path::PathBuf, Reason> {
        os_path(&self.location(rel)?).map_err(|error| reason_of(error.kind))
    }
}

impl SideSource for StdSide {
    fn provider(&self) -> &ProviderId {
        &self.provider
    }

    fn volume(&self, _rel: &RelPath) -> u64 {
        self.volume
    }

    /// Reads the whole folder, then feeds it; a cancel ends the feed early with `Ok`, as `LocalSide`.
    fn list(
        &self,
        folder: &RelPath,
        sink: &mut dyn FnMut(SideEntry) -> ControlFlow<()>,
        cancel: &CancelToken,
    ) -> Result<(), Reason> {
        if cancel.is_cancelled() {
            return Ok(());
        }
        let path = self.path(folder)?;
        let found = walk::list_dir(&path, &AtomicBool::new(false)).map_err(|e| fs_reason(&e))?;
        for entry in found {
            if cancel.is_cancelled() || sink(side_entry(folder, entry)).is_break() {
                break;
            }
        }
        Ok(())
    }

    /// Always buffered, as `LocalSide`.
    fn open(&self, rel: &RelPath, _hint: ReadHint) -> Result<Box<dyn Read + Send>, Reason> {
        let file = File::open(self.path(rel)?).map_err(|error| io_reason(&error))?;
        Ok(Box::new(file))
    }

    fn stamp(&self, rel: &RelPath) -> Result<Stamp, Reason> {
        let found = walk::stat(&self.path(rel)?).map_err(|error| fs_reason(&error))?;
        Ok(Stamp {
            size: found.size,
            modified: found.modified_ms,
            file_id: self.file_id(rel),
        })
    }

    fn object_id(&self, _rel: &RelPath) -> Option<ObjectId> {
        None
    }

    fn file_id(&self, rel: &RelPath) -> Option<FileIdentity> {
        navo_fs::native_identity(&self.path(rel).ok()?)
            .ok()
            .flatten()
    }

    fn cache_path(&self, rel: &RelPath) -> ProviderPath {
        self.location(rel).map_or_else(
            |_| rel.as_path().clone(),
            |location| location.path().clone(),
        )
    }

    /// Windows and macOS volumes fold case by default; other systems do not.
    fn case_sensitive(&self) -> bool {
        !cfg!(any(windows, target_os = "macos"))
    }

    fn read_only(&self) -> bool {
        false
    }

    fn is_network(&self) -> bool {
        self.network
    }
}

/// The engine entry for one walked child of `folder`, named as `LocalFs` names it.
fn side_entry(folder: &RelPath, found: WalkEntry) -> SideEntry {
    SideEntry::new(
        child_rel(folder, &provider_name(&found.name)),
        match found.kind {
            FsKind::File => EntryKind::File,
            FsKind::Folder => EntryKind::Folder,
            FsKind::Link => EntryKind::Link,
        },
        found.size,
        found.modified_ms,
    )
}

/// A native name as a provider name: raw bytes on Unix, UTF-16 units on Windows.
fn provider_name(name: &std::ffi::OsStr) -> ProviderPath {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        ProviderPath::from_bytes(name.as_bytes())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        ProviderPath::from_wide(&name.encode_wide().collect::<Vec<u16>>())
    }
}

/// An unknown class counts as not network, so a failed lookup never caps a walk.
fn is_network_class(class: DriveClass) -> bool {
    // Off Windows the class is always unknown; only a Windows lookup can fail.
    if class == DriveClass::Unknown && cfg!(windows) {
        tracing::warn!("drive type unknown; a compare side is treated as not on a network");
    }
    needs_network_bound(class)
}

/// Maps a stat or listing error as `LocalFs` does: the OS code first, else the portable kind.
fn fs_reason(error: &io::Error) -> Reason {
    let kind = match error.raw_os_error() {
        #[cfg(windows)]
        Some(code) => FsErrorKind::from_windows_code(code.cast_unsigned()),
        #[cfg(not(windows))]
        Some(code) => FsErrorKind::from_errno(code),
        None => match error.kind() {
            io::ErrorKind::NotFound => FsErrorKind::NotFound,
            io::ErrorKind::PermissionDenied => FsErrorKind::AccessDenied,
            io::ErrorKind::Interrupted => FsErrorKind::Cancelled,
            _ => FsErrorKind::Unknown,
        },
    };
    reason_of(kind)
}

/// Maps an open error as `LocalSide::open` does: by portable kind only.
fn io_reason(error: &io::Error) -> Reason {
    match error.kind() {
        io::ErrorKind::NotFound => Reason::NotFound,
        io::ErrorKind::PermissionDenied => Reason::AccessDenied,
        _ => Reason::ReadError,
    }
}
