use std::fs;
use std::path::Path;

use rusty_core::{AppState, SessionState};

use crate::{backup_path, write_atomic};

pub const SESSION_MAGIC: &[u8; 16] = b"FERRITE-SESSION\n";
pub const SESSION_VERSION: u16 = 1;

#[derive(Debug, thiserror::Error)]
pub enum SessionFileError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("not a Ferrite session file")]
    BadMagic,
    #[error("session format {found} is newer than the supported {supported}")]
    TooNew { found: u16, supported: u16 },
    #[error("session checksum does not match")]
    BadChecksum,
    #[error("session does not decode: {0}")]
    Decode(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionSource {
    Main,
    Backup,
    Empty,
}

const HEADER_LEN: usize = 16 + 2 + 4 + 4;
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

pub fn write_session(path: &Path, session: &SessionState) -> Result<(), SessionFileError> {
    if matches!(
        usable(path),
        Err(SessionFileError::BadMagic
            | SessionFileError::BadChecksum
            | SessionFileError::Decode(_))
    ) {
        let mut corrupt = path.as_os_str().to_owned();
        corrupt.push(".corrupt");
        let _ = fs::rename(path, corrupt);
    }
    let body =
        postcard::to_allocvec(session).map_err(|e| SessionFileError::Decode(e.to_string()))?;
    let len = u32::try_from(body.len())
        .map_err(|_| SessionFileError::Decode("session too large".to_owned()))?;
    let mut bytes = Vec::with_capacity(HEADER_LEN + body.len());
    bytes.extend_from_slice(SESSION_MAGIC);
    bytes.extend_from_slice(&SESSION_VERSION.to_le_bytes());
    bytes.extend_from_slice(&len.to_le_bytes());
    bytes.extend_from_slice(&crc32fast::hash(&body).to_le_bytes());
    bytes.extend_from_slice(&body);
    write_atomic(path, &bytes)?;
    Ok(())
}

pub fn read_session_file(path: &Path) -> Result<SessionState, SessionFileError> {
    if fs::metadata(path)?.len() > MAX_FILE_BYTES {
        return Err(SessionFileError::Decode(
            "session file is too large".to_owned(),
        ));
    }
    let bytes = fs::read(path)?;
    let (header, body) = bytes
        .split_at_checked(HEADER_LEN)
        .ok_or(SessionFileError::BadMagic)?;
    let (magic, rest) = header.split_at(SESSION_MAGIC.len());
    if magic != SESSION_MAGIC {
        return Err(SessionFileError::BadMagic);
    }
    let version = u16::from_le_bytes([rest[0], rest[1]]);
    if version > SESSION_VERSION {
        return Err(SessionFileError::TooNew {
            found: version,
            supported: SESSION_VERSION,
        });
    }
    let len = u32::from_le_bytes([rest[2], rest[3], rest[4], rest[5]]);
    let crc = u32::from_le_bytes([rest[6], rest[7], rest[8], rest[9]]);
    if usize::try_from(len).ok() != Some(body.len()) {
        return Err(SessionFileError::Decode(format!(
            "body is {} bytes, header says {len}",
            body.len()
        )));
    }
    if crc32fast::hash(body) != crc {
        return Err(SessionFileError::BadChecksum);
    }
    postcard::from_bytes(body).map_err(|e| SessionFileError::Decode(e.to_string()))
}

#[must_use]
pub fn load_session(path: &Path) -> (Option<SessionState>, SessionSource, Vec<SessionFileError>) {
    let mut errors = Vec::new();
    for (candidate, source) in [
        (path.to_owned(), SessionSource::Main),
        (backup_path(path), SessionSource::Backup),
    ] {
        match usable(&candidate) {
            Ok(session) => return (Some(session), source, errors),
            Err(error) => errors.push(error),
        }
    }
    (None, SessionSource::Empty, errors)
}

fn usable(path: &Path) -> Result<SessionState, SessionFileError> {
    let session = read_session_file(path)?;
    AppState::from_session(session.clone()).map_err(|e| SessionFileError::Decode(e.to_string()))?;
    Ok(session)
}

#[cfg(test)]
mod tests;
