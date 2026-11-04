#![allow(
    clippy::disallowed_methods,
    reason = "write-API tests plant links, swap items and fake a Recycle Bin in temp trees"
)]
#![cfg_attr(
    windows,
    allow(dead_code, reason = "some helpers serve only the Unix swap tests")
)]

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};

use super::{RecycleError, Recycler, VerifiedItem};

pub(crate) const BIN: &str = ".recycle-test";

/// Moves each item to a `.recycle-test` folder beside it, like the Recycle Bin would.
#[derive(Default)]
pub(crate) struct TestRecycler {
    pub(crate) calls: Cell<usize>,
}

impl super::act::sealed::Sealed for TestRecycler {}

impl Recycler for TestRecycler {
    fn recycle(&self, item: &VerifiedItem) -> Result<(), RecycleError> {
        self.calls.set(self.calls.get() + 1);
        let path = item.path();
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            return Err(RecycleError::Io);
        };
        let bin = parent.join(BIN);
        fs::create_dir_all(&bin).map_err(|_| RecycleError::Io)?;
        fs::rename(path, bin.join(name)).map_err(|_| RecycleError::Io)
    }
}

/// Refuses every item, as a shell that would delete permanently must.
#[derive(Default)]
pub(crate) struct AlwaysFails {
    pub(crate) calls: Cell<usize>,
}

impl super::act::sealed::Sealed for AlwaysFails {}

impl Recycler for AlwaysFails {
    fn recycle(&self, _item: &VerifiedItem) -> Result<(), RecycleError> {
        self.calls.set(self.calls.get() + 1);
        Err(RecycleError::Shell)
    }
}

/// Reports success without moving anything.
#[derive(Default)]
pub(crate) struct Liar;

impl super::act::sealed::Sealed for Liar {}

impl Recycler for Liar {
    fn recycle(&self, _item: &VerifiedItem) -> Result<(), RecycleError> {
        Ok(())
    }
}

pub(crate) fn binned(parent: &Path, name: &str) -> PathBuf {
    parent.join(BIN).join(name)
}

pub(crate) fn write(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
}

pub(crate) fn mkdir(path: &Path) {
    fs::create_dir_all(path).unwrap();
}

pub(crate) fn rename(from: &Path, to: &Path) {
    fs::rename(from, to).unwrap();
}

/// A rename that may be refused, as Windows refuses one of a held folder.
pub(crate) fn try_rename(from: &Path, to: &Path) -> bool {
    fs::rename(from, to).is_ok()
}

pub(crate) fn remove(path: &Path) {
    fs::remove_file(path).unwrap();
}

pub(crate) fn append(path: &Path, bytes: &[u8]) {
    use std::io::Write;
    let mut file = fs::OpenOptions::new().append(true).open(path).unwrap();
    file.write_all(bytes).unwrap();
}

#[cfg(unix)]
pub(crate) fn link(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[cfg(windows)]
pub(crate) fn link(target: &Path, link: &Path) {
    let output = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(output.status.success(), "the swap needs a real junction");
}

/// Names every entry under `dir`, recursively, to prove a tree did not change.
pub(crate) fn tree(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            let meta = fs::symlink_metadata(&path).unwrap();
            if meta.is_dir() {
                stack.push(path.clone());
                out.push((path, Vec::new()));
            } else if meta.is_file() {
                let bytes = fs::read(&path).unwrap();
                out.push((path, bytes));
            } else {
                out.push((path, b"link".to_vec()));
            }
        }
    }
    out.sort();
    out
}
