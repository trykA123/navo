use std::{fmt, io};

use serde::{Serialize, Serializer};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathRole {
    OwnerFolder,
    ScratchRoot,
}

impl fmt::Display for PathRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OwnerFolder => "owner-folder",
            Self::ScratchRoot => "scratch-root",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCode {
    Io(io::ErrorKind),
    /// A fixed refusal tag; never carries a path.
    Refused(&'static str),
    ScanCapacity,
    ScanBuild,
    Serialization,
    Redacted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrivacyError {
    pub role: PathRole,
    pub code: ErrorCode,
}

impl PrivacyError {
    pub fn from_io(role: PathRole, error: io::Error) -> Self {
        Self {
            role,
            code: ErrorCode::Io(error.kind()),
        }
    }

    pub(super) fn output(code: ErrorCode) -> Self {
        Self {
            role: PathRole::ScratchRoot,
            code,
        }
    }
}

impl fmt::Display for PrivacyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:", self.role)?;
        match self.code {
            ErrorCode::Io(kind) => write!(f, "io-{kind:?}"),
            code => write!(f, "{code:?}"),
        }
    }
}

impl std::error::Error for PrivacyError {}

impl Serialize for PrivacyError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}
