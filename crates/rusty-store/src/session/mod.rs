mod keep;
mod migration;

use std::fs;
use std::io::Read;
use std::path::Path;

use rusty_core::{AppState, SessionState};

use crate::{backup_path, write_atomic};

pub const SESSION_MAGIC: &[u8; 16] = b"FERRITE-SESSION\n";
pub const SESSION_VERSION: u16 = 3;

#[derive(Debug, thiserror::Error)]
pub enum SessionFileError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("not a Navo session file")]
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
    keep::set_aside_unreadable(path, keep::FileRole::Main)?;
    keep::set_aside_unreadable(&backup_path(path), keep::FileRole::Backup)?;
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
    let mut file = fs::File::open(path)?;
    let header = read_header(&mut file)?;
    if file.metadata()?.len() > MAX_FILE_BYTES {
        return Err(SessionFileError::Decode(
            "session file is too large".to_owned(),
        ));
    }
    let mut body = Vec::new();
    file.take(MAX_FILE_BYTES).read_to_end(&mut body)?;
    if usize::try_from(header.len).ok() != Some(body.len()) {
        return Err(SessionFileError::Decode(format!(
            "body is {} bytes, header says {}",
            body.len(),
            header.len
        )));
    }
    if crc32fast::hash(&body) != header.crc {
        return Err(SessionFileError::BadChecksum);
    }
    migration::decode(header.version, &body)
}

struct Header {
    version: u16,
    len: u32,
    crc: u32,
}

fn read_header(file: &mut fs::File) -> Result<Header, SessionFileError> {
    let mut bytes = Vec::with_capacity(HEADER_LEN);
    file.take(HEADER_LEN as u64).read_to_end(&mut bytes)?;
    let (magic, rest) = bytes
        .split_at_checked(SESSION_MAGIC.len())
        .filter(|(_, rest)| rest.len() == HEADER_LEN - SESSION_MAGIC.len())
        .ok_or(SessionFileError::BadMagic)?;
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
    Ok(Header {
        version,
        len: u32::from_le_bytes([rest[2], rest[3], rest[4], rest[5]]),
        crc: u32::from_le_bytes([rest[6], rest[7], rest[8], rest[9]]),
    })
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
