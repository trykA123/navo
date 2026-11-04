//! The crate's only write API; every verb is checked by the guard and acts inside the run root.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

use super::act::{self, Expected, Kind, Recycler, Stage};
use super::ledger;
use super::resolve::{self, Resolved};
use super::stale::{self, StaleCleanup};
use super::{CheckedPath, GuardError, MARKER_FILE, OutputFolder, PathRole, ScratchRoot};

const ROLE: PathRole = PathRole::ScratchRoot;
const CHUNK: usize = 256 * 1024;

/// Every public item that changes the filesystem; a test keeps this list and the code equal.
pub const WRITE_API: [&str; 7] = [
    "create_new",
    "create_dir",
    "copy_in",
    "recycle",
    "remove_root",
    "clean_stale_runs",
    "create_report",
];

impl ScratchRoot {
    /// Creates a new file at `rel`; an existing entry is never opened or truncated.
    pub fn create_new(&self, rel: &Path) -> Result<File, GuardError> {
        self.create_file(rel).map(|(file, _)| file)
    }

    /// Creates one directory at `rel`; its parent must already exist inside the root.
    pub fn create_dir(&self, rel: &Path) -> Result<(), GuardError> {
        let path = self.target_path(rel)?;
        act::create_at(&self.pin, rel, &path, Kind::Dir).map(|_| ())
    }

    /// Streams `src` into a new file at `rel` and returns the byte count; a BLAKE3
    /// difference between the bytes read and the file written removes the copy.
    pub fn copy_in(&self, src: &Path, rel: &Path) -> Result<u64, GuardError> {
        let source_role = PathRole::CopySource;
        let mut source = File::open(src).map_err(|_| GuardError::Io(source_role))?;
        let metadata = source.metadata().map_err(|_| GuardError::Io(source_role))?;
        if !metadata.is_file() {
            return Err(GuardError::InvalidPath(source_role));
        }
        let (mut dest, created) = self.create_file(rel)?;
        let result = copy_verified(&mut source, &mut dest);
        if result.is_err() {
            drop(dest);
            let _ = act::discard(&created.expected(self.path()));
        }
        result
    }

    /// Moves the item at `rel` to the Recycle Bin through `recycler`; never a hard delete.
    pub fn recycle(&self, rel: &Path, recycler: &dyn Recycler) -> Result<(), GuardError> {
        let checked = self.target(rel)?;
        let identity = act::capture(&checked, ROLE)?;
        let path = checked.resolved.path.clone();
        drop(checked);
        let expected = Expected {
            container: self.path(),
            path: &path,
            identity,
            role: ROLE,
        };
        act::recycle(&expected, recycler)
    }

    /// Recycles the whole root, only when its marker holds this run's token.
    pub fn remove_root(self, recycler: &dyn Recycler) -> Result<(), GuardError> {
        if stale::read_marker(self.path(), ROLE)? != self.token {
            return Err(GuardError::Marker(ROLE));
        }
        let identity = act::confirm_handle(&self.pin, self.path(), ROLE)?;
        let ScratchRoot { guard, pin, token } = self;
        let Resolved {
            path,
            _handles: mut ancestors,
        } = guard.root;
        let parent = path.parent().ok_or(GuardError::InvalidPath(ROLE))?;
        drop(ancestors.pop());
        let expected = Expected {
            container: parent,
            path: &path,
            identity,
            role: ROLE,
        };
        act::recycle_pinned(&expected, Some(pin), recycler)?;
        // A token left listed for a recycled run is harmless.
        let _ = ledger::remove(parent, &token, ROLE);
        Ok(())
    }

    /// Recycles older `run-*` folders beside this root whose marker token is in the ledger;
    /// anything else stays untouched.
    pub fn clean_stale_runs(
        &self,
        parent: &Path,
        recycler: &dyn Recycler,
    ) -> Result<StaleCleanup, GuardError> {
        stale::clean(self.path(), parent, recycler)
    }

    fn target(&self, rel: &Path) -> Result<CheckedPath<'_>, GuardError> {
        let plain = rel
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
        if rel.as_os_str().is_empty() || !plain || resolve::equal(rel, Path::new(MARKER_FILE)) {
            return Err(GuardError::InvalidPath(ROLE));
        }
        self.check(&self.path().join(rel))
    }

    /// The checked final path of `rel`; the check's own handles are closed again.
    fn target_path(&self, rel: &Path) -> Result<PathBuf, GuardError> {
        Ok(self.target(rel)?.resolved.path.clone())
    }

    fn create_file(&self, rel: &Path) -> Result<(File, Created), GuardError> {
        let path = self.target_path(rel)?;
        let made = act::create_at(&self.pin, rel, &path, Kind::File)?;
        let identity = made.identity;
        Ok((made.handle, Created { path, identity }))
    }
}

impl OutputFolder {
    /// Creates the new file `name` directly in the output folder; an existing entry is never
    /// opened or truncated.
    pub fn create_report(&self, name: &Path) -> Result<File, GuardError> {
        let role = PathRole::OutputFolder;
        let mut parts = name.components();
        if !matches!(
            (parts.next(), parts.next()),
            (Some(Component::Normal(_)), None)
        ) {
            return Err(GuardError::InvalidPath(role));
        }
        let path = self.guard.check(&self.path().join(name))?.resolved.path;
        act::create_at(&self.pin, name, &path, Kind::File).map(|made| made.handle)
    }
}

struct Created {
    path: PathBuf,
    identity: act::Identity,
}

impl Created {
    fn expected<'a>(&'a self, root: &'a Path) -> Expected<'a> {
        Expected {
            container: root,
            path: &self.path,
            identity: self.identity,
            role: ROLE,
        }
    }
}

fn copy_verified(source: &mut File, dest: &mut File) -> Result<u64, GuardError> {
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; CHUNK];
    let mut total = 0_u64;
    loop {
        let read = read_some(source, &mut buffer, PathRole::CopySource)?;
        let Some(bytes) = buffer.get(..read).filter(|bytes| !bytes.is_empty()) else {
            break;
        };
        hasher.update(bytes);
        dest.write_all(bytes).map_err(|_| GuardError::Io(ROLE))?;
        let count = u64::try_from(read).map_err(|_| GuardError::Io(ROLE))?;
        total = total.checked_add(count).ok_or(GuardError::Io(ROLE))?;
    }
    dest.sync_all().map_err(|_| GuardError::Io(ROLE))?;
    act::hook(Stage::BeforeHashCheck);
    dest.seek(SeekFrom::Start(0))
        .map_err(|_| GuardError::Io(ROLE))?;
    if hash_all(dest, &mut buffer)? != hasher.finalize() {
        return Err(GuardError::Mismatch(ROLE));
    }
    Ok(total)
}

fn hash_all(file: &mut File, buffer: &mut [u8]) -> Result<blake3::Hash, GuardError> {
    let mut hasher = blake3::Hasher::new();
    loop {
        let read = read_some(file, buffer, ROLE)?;
        let Some(bytes) = buffer.get(..read).filter(|bytes| !bytes.is_empty()) else {
            return Ok(hasher.finalize());
        };
        hasher.update(bytes);
    }
}

fn read_some(file: &mut File, buffer: &mut [u8], role: PathRole) -> Result<usize, GuardError> {
    loop {
        match file.read(buffer) {
            Ok(read) => return Ok(read),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return Err(GuardError::Io(role)),
        }
    }
}

/// Exclusive create; on Windows the handle can also delete the item it made.
#[allow(
    clippy::disallowed_methods,
    reason = "exclusive create of a guard-checked path"
)]
pub(super) fn open_new(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE};
        use windows_sys::Win32::Storage::FileSystem::DELETE;
        options.access_mode(GENERIC_READ | GENERIC_WRITE | DELETE);
    }
    options.open(path)
}

#[allow(
    clippy::disallowed_methods,
    reason = "single-level create of a guard-checked path"
)]
pub(super) fn make_dir(path: &Path) -> io::Result<()> {
    std::fs::create_dir(path)
}

#[allow(
    clippy::disallowed_methods,
    reason = "replaces the tool's own ledger with a fresh file the guard just created beside it"
)]
pub(super) fn replace_ledger(temporary: &Path, ledger: &Path) -> io::Result<()> {
    std::fs::rename(temporary, ledger)
}

#[cfg(not(windows))]
#[allow(
    clippy::disallowed_methods,
    reason = "read-only handle to the item itself, never a link target"
)]
pub(super) fn open_path_handle(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    #[cfg(target_os = "linux")]
    let flags = libc::O_PATH | libc::O_NOFOLLOW;
    #[cfg(not(target_os = "linux"))]
    let flags = libc::O_NOFOLLOW;
    OpenOptions::new().read(true).custom_flags(flags).open(path)
}

#[cfg(not(windows))]
#[allow(
    clippy::disallowed_methods,
    reason = "removes a file this run created, after handle verification"
)]
pub(super) fn remove_verified_file(path: &Path) -> io::Result<()> {
    std::fs::remove_file(path)
}

#[cfg(not(windows))]
#[allow(
    clippy::disallowed_methods,
    reason = "removes an empty folder this run created, after handle verification"
)]
pub(super) fn remove_verified_dir(path: &Path) -> io::Result<()> {
    std::fs::remove_dir(path)
}
