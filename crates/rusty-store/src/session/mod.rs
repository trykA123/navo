use std::path::Path;

use rusty_core::SessionState;

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

#[allow(
    unused_variables,
    reason = "stub until the session file is implemented"
)]
pub fn write_session(path: &Path, session: &SessionState) -> Result<(), SessionFileError> {
    todo!()
}

#[allow(
    unused_variables,
    reason = "stub until the session file is implemented"
)]
pub fn read_session_file(path: &Path) -> Result<SessionState, SessionFileError> {
    todo!()
}

#[allow(
    unused_variables,
    reason = "stub until the session file is implemented"
)]
#[must_use]
pub fn load_session(path: &Path) -> (Option<SessionState>, SessionSource, Vec<SessionFileError>) {
    todo!()
}

#[cfg(test)]
mod tests;
