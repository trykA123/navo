use std::fmt;
use std::sync::Mutex;

mod names;
mod path;
mod salt;
mod text;

use names::NetworkNames;
pub(crate) use path::is_allowed_extension;
pub use salt::Salt;

#[derive(Debug, Clone, Default)]
pub struct Identity {
    pub user: Option<String>,
    pub computer: Option<String>,
    pub domain: Option<String>,
    pub profile: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveKind {
    Local,
    Network,
    Removable,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    File,
    Folder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthClass {
    Short,
    Medium,
    Long,
}

#[derive(Clone, PartialEq, Eq)]
pub struct RedactedPath {
    text: String,
    drive: DriveKind,
    depth: usize,
    length_class: LengthClass,
    long: bool,
    non_ascii: bool,
}

impl RedactedPath {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn drive(&self) -> DriveKind {
        self.drive
    }

    #[must_use]
    pub fn depth(&self) -> usize {
        self.depth
    }

    #[must_use]
    pub fn length_class(&self) -> LengthClass {
        self.length_class
    }

    #[must_use]
    pub fn is_long(&self) -> bool {
        self.long
    }

    #[must_use]
    pub fn has_non_ascii(&self) -> bool {
        self.non_ascii
    }
}

impl fmt::Debug for RedactedPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Display for RedactedPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

pub struct Redactor {
    salt: Salt,
    identity: Identity,
    servers: Mutex<NetworkNames>,
}

impl Redactor {
    #[must_use]
    pub fn new(salt: Salt, identity: Identity) -> Self {
        Self {
            salt,
            identity,
            servers: Mutex::new(NetworkNames::default()),
        }
    }
}

#[cfg(test)]
mod tests;
