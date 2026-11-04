//! Raw std changes to a test's own temp tree; the only writes this module tree makes.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[allow(
    clippy::disallowed_methods,
    reason = "mutates only the test's temp tree"
)]
pub(super) fn add_file(path: &Path) -> io::Result<()> {
    fs::write(path, b"temp")
}

#[allow(
    clippy::disallowed_methods,
    reason = "mutates only the test's temp tree"
)]
pub(super) fn remove_file(path: &Path) -> io::Result<()> {
    fs::remove_file(path)
}

#[allow(
    clippy::disallowed_methods,
    reason = "mutates only the test's temp tree"
)]
pub(super) fn rename(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

#[allow(
    clippy::disallowed_methods,
    reason = "mutates only the test's temp tree"
)]
pub(super) fn make_dir(path: &Path) -> io::Result<()> {
    fs::create_dir(path)
}

#[allow(
    clippy::disallowed_methods,
    reason = "mutates only the test's temp tree"
)]
pub(super) fn hard_link(from: &Path, to: &Path) -> io::Result<()> {
    fs::hard_link(from, to)
}

#[allow(
    clippy::disallowed_methods,
    reason = "mutates only the test's temp tree"
)]
pub(super) fn append(path: &Path) -> io::Result<()> {
    OpenOptions::new().append(true).open(path)?.write_all(b"+")
}

#[allow(
    clippy::disallowed_methods,
    reason = "mutates only the test's temp tree"
)]
pub(super) fn set_mtime(path: &Path, time: SystemTime) -> io::Result<()> {
    OpenOptions::new()
        .write(true)
        .open(path)?
        .set_modified(time)
}

/// Moves the mtime by a fixed, recognisable amount.
pub(super) fn touch(path: &Path) -> io::Result<()> {
    set_mtime(path, UNIX_EPOCH + Duration::from_secs(1_000_000_000))
}

#[allow(
    clippy::disallowed_methods,
    reason = "mutates only the test's temp tree"
)]
pub(super) fn toggle_readonly(path: &Path) -> io::Result<()> {
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_readonly(!permissions.readonly());
    fs::set_permissions(path, permissions)
}

/// Flips one byte, keeps the size and restores the mtime.
#[allow(
    clippy::disallowed_methods,
    reason = "mutates only the test's temp tree"
)]
pub(super) fn flip_byte_keep_mtime(path: &Path) -> io::Result<()> {
    let modified = fs::metadata(path)?.modified()?;
    let mut bytes = fs::read(path)?;
    bytes[0] ^= 1;
    fs::write(path, bytes)?;
    set_mtime(path, modified)
}

#[cfg(unix)]
#[allow(
    clippy::disallowed_methods,
    reason = "mutates only the test's temp tree"
)]
pub(super) fn retarget(link: &Path, target: &str) -> io::Result<()> {
    fs::remove_file(link)?;
    std::os::unix::fs::symlink(target, link)
}
