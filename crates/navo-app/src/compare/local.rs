//! A local folder as a compare side.

use std::fs::File;
use std::io::{self, Read};
use std::ops::ControlFlow;

use navo_compare::model::{
    CancelToken, ObjectId, ReadHint, Reason, RelPath, SideEntry, SideSource, Stamp,
};
use navo_core::{FileIdentity, Location, ProviderId, ProviderPath};
use navo_fs::{CancelToken as FsCancel, EntryKind as FsKind, LocalFs, Provider, os_path};

use super::entry::{feed, reason_of, segments};

/// A local folder. Names are matched as text, so a name that is not valid text cannot be opened.
pub(crate) struct LocalSide {
    root: Location,
    fs: LocalFs,
    provider: ProviderId,
    volume: u64,
}

impl LocalSide {
    /// Opens a side on `root`; fails with a reason when it is not a readable folder.
    pub(crate) fn new(root: Location) -> Result<Self, Reason> {
        let fs = LocalFs::new();
        let found = fs.metadata(&root).map_err(|error| reason_of(error.kind))?;
        let is_folder = found.kind == FsKind::Folder || found.attributes.link_to_folder;
        if !is_folder {
            return Err(Reason::Unsupported);
        }
        let volume = os_path(&root)
            .ok()
            .and_then(|path| navo_fs::native_identity(&path).ok().flatten())
            .map_or(0, |identity| identity.volume);
        Ok(Self {
            root,
            fs,
            provider: ProviderId::local(),
            volume,
        })
    }

    fn location(&self, rel: &RelPath) -> Result<Location, Reason> {
        let mut location = self.root.clone();
        for part in segments(rel) {
            location = location
                .join(&ProviderPath::from_text(&part))
                .map_err(|_| Reason::Unsupported)?;
        }
        Ok(location)
    }
}

impl SideSource for LocalSide {
    fn provider(&self) -> &ProviderId {
        &self.provider
    }

    fn volume(&self, _rel: &RelPath) -> u64 {
        self.volume
    }

    fn list(
        &self,
        folder: &RelPath,
        sink: &mut dyn FnMut(SideEntry) -> ControlFlow<()>,
        cancel: &CancelToken,
    ) -> Result<(), Reason> {
        let location = self.location(folder)?;
        let mut batch = |batch| feed(folder, batch, sink, cancel);
        match self.fs.list(&location, &mut batch, &FsCancel::new()) {
            Ok(()) => Ok(()),
            Err(error) => Err(reason_of(error.kind)),
        }
    }

    /// Always buffered: `Map` reads the same way (S4 found no gain from maps).
    fn open(&self, rel: &RelPath, _hint: ReadHint) -> Result<Box<dyn Read + Send>, Reason> {
        let path = os_path(&self.location(rel)?).map_err(|error| reason_of(error.kind))?;
        let file = File::open(path).map_err(|error| io_reason(&error))?;
        Ok(Box::new(file))
    }

    fn stamp(&self, rel: &RelPath) -> Result<Stamp, Reason> {
        let location = self.location(rel)?;
        let found = self
            .fs
            .metadata(&location)
            .map_err(|error| reason_of(error.kind))?;
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
        let path = os_path(&self.location(rel).ok()?).ok()?;
        navo_fs::native_identity(&path).ok().flatten()
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
}

fn io_reason(error: &io::Error) -> Reason {
    match error.kind() {
        io::ErrorKind::NotFound => Reason::NotFound,
        io::ErrorKind::PermissionDenied => Reason::AccessDenied,
        _ => Reason::ReadError,
    }
}
