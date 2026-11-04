//! Tester probe. `navo.exe --version` prints no feature, so the probe reads the exe bytes
//! for `MARKER`: the usage text that only a `tester` build contains (navo-app `cli.rs`).

use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

pub const MARKER: &[u8] = b"--bench index [--json]";
const EXE_NAME: &str = if cfg!(windows) { "navo.exe" } else { "navo" };
const CHUNK: usize = 1 << 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbeError {
    /// No navo exe next to the bench exe.
    Missing,
    /// Present, but built without the tester feature.
    NotTester,
    Unreadable(io::ErrorKind),
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => f.write_str("probe:navo-missing"),
            Self::NotTester => f.write_str("probe:not-a-tester-build"),
            Self::Unreadable(kind) => write!(f, "probe:io-{kind:?}"),
        }
    }
}

impl std::error::Error for ProbeError {}

/// Finds navo next to the running bench exe and checks it is a tester build.
pub fn probe_beside_current() -> Result<PathBuf, ProbeError> {
    let exe = std::env::current_exe().map_err(|error| ProbeError::Unreadable(error.kind()))?;
    let folder = exe.parent().ok_or(ProbeError::Missing)?;
    probe_in(folder)
}

pub fn probe_in(folder: &Path) -> Result<PathBuf, ProbeError> {
    let navo = folder.join(EXE_NAME);
    let file = File::open(&navo).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => ProbeError::Missing,
        kind => ProbeError::Unreadable(kind),
    })?;
    if contains_marker(file).map_err(|error| ProbeError::Unreadable(error.kind()))? {
        Ok(navo)
    } else {
        Err(ProbeError::NotTester)
    }
}

fn contains_marker(mut reader: impl Read) -> io::Result<bool> {
    let keep = MARKER.len() - 1;
    let mut buffer = vec![0_u8; CHUNK + keep];
    let mut carried = 0;
    loop {
        let read = reader.read(&mut buffer[carried..])?;
        if read == 0 {
            return Ok(false);
        }
        let filled = carried + read;
        if buffer[..filled].windows(MARKER.len()).any(|w| w == MARKER) {
            return Ok(true);
        }
        carried = keep.min(filled);
        buffer.copy_within(filled - carried..filled, 0);
    }
}
