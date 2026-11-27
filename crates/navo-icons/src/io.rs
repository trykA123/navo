//! Bounded file readers and filename checks for untrusted icon inputs.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;

/// Why a bounded file read failed.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    /// The file contained more bytes than the configured limit.
    #[error("file exceeds size limit")]
    TooLarge,
    /// An underlying filesystem or I/O error occurred.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Reads at most `cap` bytes from `path`, refusing symlinks and non-regular files.
pub fn read_capped(path: &Path, cap: u64) -> Result<Vec<u8>, ReadError> {
    let link_meta = fs::symlink_metadata(path)?;
    if link_meta.file_type().is_symlink() || !link_meta.file_type().is_file() {
        return Err(ReadError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to read symlink or non-regular file",
        )));
    }
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(ReadError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to read symlink or non-regular file",
        )));
    }
    let mut bytes = Vec::new();
    file.take(cap.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > cap {
        return Err(ReadError::TooLarge);
    }
    Ok(bytes)
}

/// True for a safe ASCII pack filename `[a-z0-9._-]+.nvic` that is not a Windows device name.
pub(crate) fn is_pack_filename(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".nvic") else {
        return false;
    };
    if stem.is_empty()
        || stem.starts_with('.')
        || stem.contains("..")
        || !stem
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-'))
    {
        return false;
    }
    let base = stem.split('.').next().unwrap_or("");
    !matches!(
        base,
        "con"
            | "prn"
            | "aux"
            | "nul"
            | "com1"
            | "com2"
            | "com3"
            | "com4"
            | "com5"
            | "com6"
            | "com7"
            | "com8"
            | "com9"
            | "lpt1"
            | "lpt2"
            | "lpt3"
            | "lpt4"
            | "lpt5"
            | "lpt6"
            | "lpt7"
            | "lpt8"
            | "lpt9"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn read_capped_refuses_symlink() {
        use std::os::unix::fs::symlink;

        let dir =
            std::env::temp_dir().join(format!("navo-icons-io-symlink-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let real = dir.join("real.txt");
        let link = dir.join("link.txt");
        std::fs::write(&real, b"hello").expect("write real");
        symlink(&real, &link).expect("create symlink");

        let err = read_capped(&link, 1024).expect_err("symlink must be refused");
        assert!(
            matches!(err, ReadError::Io(ref io_err) if io_err.kind() == std::io::ErrorKind::InvalidInput),
            "expected InvalidInput for symlink, got {err:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pack_filename_enforces_strict_allowlist() {
        for good in ["fluent-1.0.0.nvic", "fluent_filled-2.0.nvic", "a.nvic"] {
            assert!(is_pack_filename(good), "{good} should be accepted");
        }
        for bad in [
            "",
            ".nvic",
            ".hidden.nvic",
            "a..b.nvic",
            "Fluent-1.0.0.nvic",
            "pack.txt",
            "pack:ads.nvic",
            "../pack.nvic",
            "con.nvic",
            "CON.nvic",
            "nul.1.nvic",
            "com1.nvic",
            "lpt9.nvic",
        ] {
            assert!(!is_pack_filename(bad), "{bad} should be rejected");
        }
    }
}
