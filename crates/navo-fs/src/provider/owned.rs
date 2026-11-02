use std::io::Read;

use navo_core::{FileIdentity, FsError, Location};

use super::CancelToken;

pub struct OwnedCopy<'a> {
    pub source: &'a Location,
    pub target: &'a Location,
    pub created: &'a mut dyn FnMut(FileIdentity) -> Result<(), FsError>,
}

pub struct CopyControl<'a> {
    pub progress: &'a mut dyn FnMut(u64),
    pub cancel: &'a CancelToken,
}

/// Bytes from another provider for a new file at `target`; `created` sees its identity before any byte.
pub struct StreamWrite<'a> {
    pub bytes: &'a mut dyn Read,
    pub source: &'a Location,
    pub target: &'a Location,
    pub created: Option<&'a mut dyn FnMut(FileIdentity) -> Result<(), FsError>>,
}
