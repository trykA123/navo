//! Owner-only temporary init files for shell prompt hooks. Each hook gets a private
//! directory holding one init file; the whole directory is removed when the last owner
//! drops it (the first trusted prompt marker, or session end).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// A private temporary directory holding one shell init file, removed on drop.
#[derive(Debug)]
pub(crate) struct TempInit {
    dir: PathBuf,
    file: PathBuf,
}

impl TempInit {
    /// Creates a `navo-<random>` directory under the OS temp folder (`0o700` on Unix;
    /// the per-user temp ACLs on Windows) and writes each `(name, contents)` owner-only
    /// (`0o600` on Unix). Never overwrites an existing path. `file()` is the first entry.
    pub(crate) fn new(files: &[(&str, &str)]) -> io::Result<Self> {
        let dir = std::env::temp_dir().join(format!("navo-{}", random_suffix()?));
        create_private_dir(&dir)?;
        let result = write_all_private(&dir, files);
        if let Err(error) = result {
            let _ = fs::remove_dir_all(&dir);
            return Err(error);
        }
        let file = dir.join(files.first().map_or("init", |(name, _)| *name));
        Ok(Self { dir, file })
    }

    /// The init file to hand to the shell.
    pub(crate) fn file(&self) -> &Path {
        &self.file
    }

    /// The private directory (used as the temporary `ZDOTDIR` for zsh).
    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }
}

impl Drop for TempInit {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Creates the private directory: `0o700` on Unix, the per-user temp ACLs on Windows.
fn create_private_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(dir)
    }
    #[cfg(not(unix))]
    {
        fs::create_dir(dir)
    }
}

/// Writes every init file; the first entry is the one handed to the shell.
fn write_all_private(dir: &Path, files: &[(&str, &str)]) -> io::Result<()> {
    for (name, contents) in files {
        write_private(&dir.join(name), contents)?;
    }
    Ok(())
}

/// Writes a new owner-only file; fails if the path already exists.
fn write_private(file: &Path, contents: &str) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(file)?.write_all(contents.as_bytes())
}

/// 64 random bits as lowercase hex, so temp names cannot be guessed or collide.
fn random_suffix() -> io::Result<String> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
