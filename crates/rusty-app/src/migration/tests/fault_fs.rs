use std::cell::Cell;
use std::ffi::OsString;
use std::io;
use std::path::Path;

use super::super::{MigrationFs, RealFs};

#[derive(Clone, Copy, Debug)]
pub(super) enum Fault {
    None,
    FailAt(usize),
    FailTwice(usize, usize),
    CrashAt(usize),
    Deny(&'static str),
    CorruptCopies,
    SyncDir,
    #[cfg_attr(
        not(windows),
        expect(dead_code, reason = "sharing violations exist only on Windows")
    )]
    InUse,
}

pub(super) struct FaultFs {
    fault: Fault,
    writes: Cell<usize>,
}

impl FaultFs {
    pub(super) fn new(fault: Fault) -> Self {
        Self {
            fault,
            writes: Cell::new(0),
        }
    }

    pub(super) fn writes(&self) -> usize {
        self.writes.get()
    }

    fn hit(&self, what: &str, path: &Path) -> io::Result<()> {
        let n = self.writes.get();
        self.writes.set(n + 1);
        let failing = match self.fault {
            Fault::FailAt(k) => n == k,
            Fault::FailTwice(a, b) => n == a || n == b,
            Fault::CrashAt(k) => n >= k,
            _ => false,
        };
        if failing {
            return Err(io::Error::other(format!(
                "injected fault at write {n}: {what} {}",
                path.display()
            )));
        }
        Ok(())
    }
}

impl MigrationFs for FaultFs {
    fn exists(&self, path: &Path) -> bool {
        RealFs.exists(path)
    }

    fn entries(&self, path: &Path) -> io::Result<Vec<OsString>> {
        RealFs.entries(path)
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        RealFs.read(path)
    }

    fn same_bytes(&self, a: &Path, b: &Path) -> io::Result<bool> {
        RealFs.same_bytes(a, b)
    }

    fn create_dir(&self, path: &Path) -> io::Result<()> {
        self.hit("create_dir", path)?;
        RealFs.create_dir(path)
    }

    fn copy(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.hit("copy", to)?;
        RealFs.copy(from, to)?;
        if matches!(self.fault, Fault::CorruptCopies) {
            let mut bytes = std::fs::read(to)?;
            bytes.push(0);
            std::fs::write(to, bytes)?;
        }
        Ok(())
    }

    fn write_file(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        self.hit("write_file", path)?;
        RealFs.write_file(path, bytes)
    }

    fn sync_file(&self, path: &Path) -> io::Result<()> {
        self.hit("sync_file", path)
    }

    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        self.hit("sync_dir", path)?;
        if matches!(self.fault, Fault::SyncDir) {
            return Err(io::Error::from_raw_os_error(1));
        }
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.hit("rename", from)?;
        if let Fault::Deny(name) = self.fault
            && from.file_name().is_some_and(|file| file == name)
        {
            return Err(io::Error::from_raw_os_error(5));
        }
        if matches!(self.fault, Fault::InUse) {
            return Err(io::Error::from_raw_os_error(32));
        }
        RealFs.rename(from, to)
    }

    fn replace(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.hit("replace", to)?;
        RealFs.replace(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.hit("remove_file", path)?;
        RealFs.remove_file(path)
    }

    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        self.hit("remove_dir", path)?;
        RealFs.remove_dir(path)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        self.hit("remove_dir_all", path)?;
        RealFs.remove_dir_all(path)
    }
}
