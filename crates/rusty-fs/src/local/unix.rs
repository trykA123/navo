#![cfg_attr(
    target_os = "linux",
    allow(
        unsafe_code,
        reason = "Linux account names require reentrant libc lookups"
    )
)]

use std::ffi::{OsStr, OsString};
use std::fs::{self, Metadata};
use std::io;
use std::ops::ControlFlow;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusty_core::{FsError, FsErrorKind, Location, ProviderPath};

use crate::{
    ALREADY_EXISTS_CODES, Attributes, CancelToken, FileDetails, FsEntry, LinkKey, SizeEntry,
};

const OWNER_WRITE: u32 = 0o200;

#[cfg(target_os = "linux")]
const NO_HARD_LINK_CODES: [i32; 3] = [1, 38, 95];
#[cfg(not(target_os = "linux"))]
const NO_HARD_LINK_CODES: [i32; 3] = [1, 78, 45];

pub(super) fn os_path(path: &ProviderPath) -> Option<PathBuf> {
    path.to_bytes()
        .map(|bytes| PathBuf::from(OsString::from_vec(bytes)))
}

pub(super) fn entry_name(name: &OsStr) -> ProviderPath {
    ProviderPath::from_bytes(name.as_bytes())
}

pub(super) fn details(path: &Path) -> io::Result<FileDetails> {
    let metadata = fs::symlink_metadata(path)?;
    let name = path.file_name().unwrap_or(path.as_os_str());
    let mut details = FileDetails::from(super::entry(entry_name(name), name, &metadata));
    details.on_disk = Some(metadata.blocks() * 512);
    details.created_ms = created_ms(path, &metadata);
    details.accessed_ms = timestamp_ms(metadata.accessed());
    #[cfg(target_os = "linux")]
    read_security(&mut details, &metadata);
    Ok(details)
}

#[cfg(target_os = "linux")]
fn read_security(details: &mut FileDetails, metadata: &Metadata) {
    let names = account_name(metadata.uid(), false)
        .map_err(|error| security_note("Owner", error))
        .and_then(|owner| {
            account_name(metadata.gid(), true)
                .map(|group| (owner, group))
                .map_err(|error| security_note("Group", error))
        });
    let (owner, group) = match names {
        Ok(names) => names,
        Err(note) => {
            details.security_note = Some(note);
            return;
        }
    };
    details.owner = Some(owner.clone());
    details.acl = [owner, group, "Others".into()]
        .into_iter()
        .zip([6, 3, 0])
        .map(|(who, shift)| {
            let mode = metadata.mode() >> shift;
            crate::AclRow {
                who,
                rights: [(4, 'r'), (2, 'w'), (1, 'x')]
                    .map(|(bit, letter)| if mode & bit != 0 { letter } else { '-' })
                    .into_iter()
                    .collect(),
            }
        })
        .collect();
}

#[cfg(target_os = "linux")]
fn security_note(subject: &str, error: io::Error) -> String {
    if error.kind() == io::ErrorKind::PermissionDenied {
        "Access denied".into()
    } else {
        format!("{subject} lookup failed: {error}")
    }
}

#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Default)]
struct Passwd {
    name: *mut std::ffi::c_char,
    password: *mut std::ffi::c_char,
    uid: u32,
    gid: u32,
    gecos: *mut std::ffi::c_char,
    directory: *mut std::ffi::c_char,
    shell: *mut std::ffi::c_char,
}

#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Default)]
struct Group {
    name: *mut std::ffi::c_char,
    password: *mut std::ffi::c_char,
    gid: u32,
    members: *mut *mut std::ffi::c_char,
}

#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn getpwuid_r(
        uid: u32,
        record: *mut Passwd,
        buffer: *mut std::ffi::c_char,
        length: usize,
        result: *mut *mut Passwd,
    ) -> std::ffi::c_int;
    fn getgrgid_r(
        gid: u32,
        record: *mut Group,
        buffer: *mut std::ffi::c_char,
        length: usize,
        result: *mut *mut Group,
    ) -> std::ffi::c_int;
}

#[cfg(target_os = "linux")]
fn account_name(id: u32, is_group: bool) -> io::Result<String> {
    let mut buffer = vec![0_u8; 1_024];
    loop {
        let (code, name) = if is_group {
            let mut record = Group::default();
            let mut result = std::ptr::null_mut();
            // SAFETY: the Linux group layout and writable buffer match getgrgid_r's ABI.
            let code = unsafe {
                getgrgid_r(
                    id,
                    &raw mut record,
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                    &raw mut result,
                )
            };
            (code, (!result.is_null()).then_some(record.name))
        } else {
            let mut record = Passwd::default();
            let mut result = std::ptr::null_mut();
            // SAFETY: the Linux passwd layout and writable buffer match getpwuid_r's ABI.
            let code = unsafe {
                getpwuid_r(
                    id,
                    &raw mut record,
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                    &raw mut result,
                )
            };
            (code, (!result.is_null()).then_some(record.name))
        };
        if code == rustix::io::Errno::RANGE.raw_os_error() && buffer.len() < 1_048_576 {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        if code != 0 {
            return Err(io::Error::from_raw_os_error(code));
        }
        let name = name
            .filter(|name| !name.is_null())
            .ok_or(io::ErrorKind::NotFound)?;
        // SAFETY: a successful reentrant lookup returned a terminated name in the live buffer.
        return Ok(unsafe { std::ffi::CStr::from_ptr(name) }
            .to_string_lossy()
            .into_owned());
    }
}

fn timestamp_ms(time: io::Result<SystemTime>) -> Option<i64> {
    match time.ok()?.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_millis()).ok(),
        Err(before) => i64::try_from(before.duration().as_millis())
            .ok()
            .map(i64::wrapping_neg),
    }
}

#[cfg(target_os = "linux")]
fn created_ms(path: &Path, _metadata: &Metadata) -> Option<i64> {
    use rustix::fs::{AtFlags, CWD, StatxFlags, statx};

    let stat = statx(
        CWD,
        path,
        AtFlags::SYMLINK_NOFOLLOW | AtFlags::NO_AUTOMOUNT,
        StatxFlags::BTIME,
    )
    .ok()?;
    if stat.stx_mask & StatxFlags::BTIME.bits() == 0 {
        return None;
    }
    stat.stx_btime
        .tv_sec
        .checked_mul(1_000)?
        .checked_add(i64::from(stat.stx_btime.tv_nsec / 1_000_000))
}

#[cfg(not(target_os = "linux"))]
fn created_ms(_path: &Path, metadata: &Metadata) -> Option<i64> {
    timestamp_ms(metadata.created())
}

pub(super) fn list_sizes(
    batch_size: usize,
    folder: &Location,
    sink: &mut dyn FnMut(SizeEntry) -> ControlFlow<()>,
    cancel: &CancelToken,
) -> Result<(), FsError> {
    let path = super::os_path(folder)?;
    let mut children = fs::read_dir(&path)
        .map_err(|e| super::open_failure(&e, &path, folder))?
        .filter_map(|child| {
            super::std_entry_with(child, folder, |entry, metadata, _| {
                size_entry(entry, metadata)
            })
        })
        .peekable();
    loop {
        let entries: Vec<_> = children
            .by_ref()
            .take(batch_size)
            .collect::<Result<_, _>>()?;
        let last = children.peek().is_none();
        if cancel.is_cancelled() {
            return Err(super::error(FsErrorKind::Cancelled, folder));
        }
        if entries.into_iter().try_for_each(&mut *sink).is_break() || last {
            return Ok(());
        }
    }
}

pub(super) fn size_entry(entry: FsEntry, metadata: Option<&Metadata>) -> SizeEntry {
    SizeEntry {
        name: entry.name,
        kind: entry.kind,
        size: entry.size,
        on_disk: metadata.map(|metadata| metadata.blocks() * 512),
        link: metadata
            .filter(|metadata| metadata.nlink() > 1)
            .map(|metadata| LinkKey(u128::from(metadata.ino()), metadata.dev())),
    }
}

pub(super) fn attributes(name: &OsStr, metadata: &Metadata) -> Attributes {
    Attributes {
        hidden: name.as_bytes().starts_with(b"."),
        system: false,
        readonly: metadata.permissions().mode() & OWNER_WRITE == 0,
        ..Attributes::default()
    }
}

pub(super) fn name_attributes(name: &OsStr) -> Attributes {
    Attributes {
        hidden: name.as_bytes().starts_with(b"."),
        ..Attributes::default()
    }
}

pub(super) fn error_kind(code: i32) -> FsErrorKind {
    FsErrorKind::from_errno(code)
}

pub(super) fn volume_serial(folder: &Path) -> io::Result<u64> {
    fs::metadata(folder).map(|metadata| metadata.dev())
}

pub(super) fn replace(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

#[cfg(target_os = "linux")]
pub(super) fn rename_atomic(from: &Path, to: &Path) -> io::Result<()> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};
    atomic_result(
        renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE).map_err(io::Error::from),
    )
}

#[cfg(target_os = "linux")]
pub(super) fn atomic_result(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(error) if matches!(error.raw_os_error(), Some(22 | 38 | 95)) => {
            Err(io::ErrorKind::Unsupported.into())
        }
        other => other,
    }
}

#[cfg(not(target_os = "linux"))]
pub(super) fn rename_atomic(_from: &Path, _to: &Path) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
}

#[cfg(target_os = "linux")]
pub(super) fn rename_new(from: &Path, to: &Path) -> io::Result<()> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};
    use rustix::io::Errno;

    match renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE) {
        Err(Errno::INVAL | Errno::NOSYS) => link_then_unlink(from, to),
        other => other.map_err(io::Error::from),
    }
}

#[cfg(not(target_os = "linux"))]
pub(super) fn rename_new(from: &Path, to: &Path) -> io::Result<()> {
    link_then_unlink(from, to)
}

fn link_then_unlink(from: &Path, to: &Path) -> io::Result<()> {
    move_without_replace(from, to, |old, new| fs::hard_link(old, new))
}

pub(super) fn move_without_replace(
    from: &Path,
    to: &Path,
    link: impl Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    move_with(from, to, link, |old| fs::remove_file(old))
}

pub(super) fn move_with(
    from: &Path,
    to: &Path,
    link: impl Fn(&Path, &Path) -> io::Result<()>,
    unlink: impl Fn(&Path) -> io::Result<()>,
) -> io::Result<()> {
    if fs::symlink_metadata(from)?.is_dir() {
        return rename_if_absent(from, to);
    }
    match link(from, to) {
        Ok(()) => {}
        Err(error) if lacks_hard_links(&error) => return rename_if_absent(from, to),
        Err(error) => return Err(error),
    }
    unlink(from).or_else(|error| settle_unlink(from, to, error))
}

fn settle_unlink(from: &Path, to: &Path, error: io::Error) -> io::Result<()> {
    match fs::symlink_metadata(from) {
        Err(gone) if gone.kind() == io::ErrorKind::NotFound => Ok(()),
        Ok(old) if fs::symlink_metadata(to).is_ok_and(|new| same_inode(&old, &new)) => {
            let _ = fs::remove_file(to);
            Err(error)
        }
        _ => Err(error),
    }
}

fn same_inode(left: &Metadata, right: &Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino()
}

fn lacks_hard_links(error: &io::Error) -> bool {
    error
        .raw_os_error()
        .is_some_and(|code| NO_HARD_LINK_CODES.contains(&code))
}

fn rename_if_absent(from: &Path, to: &Path) -> io::Result<()> {
    match fs::symlink_metadata(to) {
        Ok(_) => Err(io::Error::from_raw_os_error(ALREADY_EXISTS_CODES[0])),
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::rename(from, to),
        Err(error) => Err(error),
    }
}
