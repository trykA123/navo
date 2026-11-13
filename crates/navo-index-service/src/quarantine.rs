//! Durable rebuild markers; dormant until T8 connects recovery and promotion.
#![allow(dead_code, reason = "T8 activates the quarantine API")]
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Identity {
    pub serial: u64,
    pub journal: u64,
    pub cursor: i64,
    pub checksum: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Marker {
    Absent,
    SearchOnly,
    Complete,
}

/// Appends a fixed marker suffix without changing the snapshot extension.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    name.into()
}
/// Encodes a versioned record bound to the canonical snapshot identity.
fn record(state: &str, id: Identity) -> String {
    format!(
        "NAVO-QUARANTINE-1:{state}:{}:{}:{}:{}\n",
        id.serial, id.journal, id.cursor, id.checksum
    )
}
/// Persists active quarantine for this snapshot; I/O failures stop preparation.
pub(crate) fn write_active(path: &Path, id: Identity) -> io::Result<()> {
    write_with(path, &record("active", id), &mut |_| Ok(()))
}
/// Certifies a durable canonical identity; I/O failures retain quarantine.
pub(crate) fn write_complete(path: &Path, id: Identity) -> io::Result<()> {
    write_with(path, &record("complete", id), &mut |_| Ok(()))
}
/// Writes a record with numbered sync, replacement and final-flush fault boundaries.
fn write_with(
    path: &Path,
    bytes: &str,
    step: &mut impl FnMut(u8) -> io::Result<()>,
) -> io::Result<()> {
    let temporary = sibling(path, ".rebuilding.tmp");
    let pending = sibling(path, ".rebuilding.pending");
    if temporary.try_exists()? {
        if pending.try_exists()? {
            fs::remove_file(&temporary)?;
        } else {
            fs::File::options()
                .write(true)
                .open(&temporary)?
                .sync_all()?;
            navo_fs::native_replace_durable(&temporary, &pending)?;
        }
    }
    let mut file = fs::File::options()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(bytes.as_bytes())?;
    step(1)?;
    file.sync_all()?;
    drop(file);
    navo_fs::native_replace_durable(&temporary, &pending)?;
    fs::hard_link(&pending, &temporary)?;
    step(2)?;
    navo_fs::native_replace_durable(&temporary, &sibling(path, ".rebuilding"))?;
    step(3)?;
    fs::remove_file(pending)
}
/// Classifies bounded marker bytes against a validated canonical identity; I/O errors fail closed.
pub(crate) fn read(path: &Path, id: Identity) -> io::Result<Marker> {
    for suffix in [".rebuilding.tmp", ".rebuilding.pending"] {
        if sibling(path, suffix).try_exists()? {
            return Ok(Marker::SearchOnly);
        }
    }
    let file = match fs::File::open(sibling(path, ".rebuilding")) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Marker::Absent),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(128).read_to_end(&mut bytes)?;
    Ok(if bytes == record("complete", id).as_bytes() {
        Marker::Complete
    } else {
        Marker::SearchOnly
    })
}
/// Removes only a matching complete marker; active or damaged records are refused.
pub(crate) fn clear(path: &Path, id: Identity) -> io::Result<()> {
    match read(path, id)? {
        Marker::Absent => Ok(()),
        Marker::Complete => fs::remove_file(sibling(path, ".rebuilding")),
        Marker::SearchOnly => Err(io::ErrorKind::InvalidData.into()),
    }
}

#[cfg(test)]
#[path = "quarantine_tests.rs"]
mod tests;
