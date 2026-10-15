use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

#[derive(Clone)]
pub struct Salt([u8; 16]);

impl Salt {
    #[must_use]
    pub fn random() -> Self {
        todo!()
    }

    #[must_use]
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        todo!()
    }
}

impl fmt::Debug for Salt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Salt(..)")
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
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
        todo!()
    }

    #[must_use]
    pub fn drive(&self) -> DriveKind {
        todo!()
    }

    #[must_use]
    pub fn depth(&self) -> usize {
        todo!()
    }

    #[must_use]
    pub fn length_class(&self) -> LengthClass {
        todo!()
    }

    #[must_use]
    pub fn is_long(&self) -> bool {
        todo!()
    }

    #[must_use]
    pub fn has_non_ascii(&self) -> bool {
        todo!()
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
    servers: Mutex<HashMap<String, usize>>,
}

impl Redactor {
    #[must_use]
    pub fn new(salt: Salt, identity: Identity) -> Self {
        todo!()
    }

    #[must_use]
    pub fn path(&self, path: &str, kind: PathKind, drive: DriveKind) -> RedactedPath {
        todo!()
    }

    #[must_use]
    pub fn text(&self, text: &str) -> String {
        todo!()
    }
}

#[cfg(test)]
mod tests;
