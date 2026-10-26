use rusty_core::{FileIdentity, FsError, Location};

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
