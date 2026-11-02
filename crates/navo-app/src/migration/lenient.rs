use std::cell::RefCell;
use std::ffi::OsString;
use std::io;
use std::path::Path;

use super::fs::MigrationFs;

pub(crate) struct LenientSync<F> {
    inner: F,
    lenient: bool,
    failure: RefCell<Option<io::Error>>,
}

impl<F: MigrationFs> LenientSync<F> {
    pub(crate) fn new(inner: F, lenient: bool) -> Self {
        Self {
            inner,
            lenient,
            failure: RefCell::new(None),
        }
    }

    pub(crate) fn into_failure(self) -> Option<io::Error> {
        self.failure.into_inner()
    }
}

impl<F: MigrationFs> MigrationFs for LenientSync<F> {
    fn exists(&self, path: &Path) -> bool {
        self.inner.exists(path)
    }

    fn entries(&self, path: &Path) -> io::Result<Vec<OsString>> {
        self.inner.entries(path)
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.inner.read(path)
    }

    fn same_bytes(&self, a: &Path, b: &Path) -> io::Result<bool> {
        self.inner.same_bytes(a, b)
    }

    fn create_dir(&self, path: &Path) -> io::Result<()> {
        self.inner.create_dir(path)
    }

    fn copy(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.inner.copy(from, to)
    }

    fn write_file(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        self.inner.write_file(path, bytes)
    }

    fn sync_file(&self, path: &Path) -> io::Result<()> {
        self.inner.sync_file(path)
    }

    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        match self.inner.sync_dir(path) {
            Err(error) if self.lenient => {
                let error = io::Error::new(
                    error.kind(),
                    format!("cannot flush {}: {error}", path.display()),
                );
                self.failure.borrow_mut().get_or_insert(error);
                Ok(())
            }
            result => result,
        }
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.inner.rename(from, to)
    }

    fn replace(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.inner.replace(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.inner.remove_file(path)
    }

    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        self.inner.remove_dir(path)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        self.inner.remove_dir_all(path)
    }
}
