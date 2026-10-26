#![allow(
    unsafe_code,
    reason = "CopyFile2 supplies a borrowed destination handle"
)]

use super::super::{error, identity, io_error};
use rusty_core::{FileIdentity, FsError, FsErrorKind, Location};
use std::fs::File;
use std::io;
use std::os::windows::io::BorrowedHandle;
use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};

pub(super) struct Registration<'a> {
    target: &'a Location,
    created: &'a mut dyn FnMut(FileIdentity) -> Result<(), FsError>,
    registered: bool,
    failure: Option<FsError>,
}

impl<'a> Registration<'a> {
    pub(super) fn new(
        target: &'a Location,
        created: &'a mut dyn FnMut(FileIdentity) -> Result<(), FsError>,
    ) -> Self {
        Self {
            target,
            created,
            registered: false,
            failure: None,
        }
    }

    pub(super) fn register(&mut self, handle: HANDLE) -> bool {
        if self.failure.is_some() {
            return false;
        }
        if self.registered {
            return true;
        }
        let result = opened_identity(handle)
            .map_err(|failure| io_error(&failure, self.target))
            .and_then(|id| id.ok_or_else(|| error(FsErrorKind::IdentityUnavailable, self.target)))
            .and_then(&mut *self.created);
        match result {
            Ok(()) => {
                self.registered = true;
                true
            }
            Err(failure) => {
                self.failure = Some(failure);
                false
            }
        }
    }

    pub(super) fn failed(&self) -> bool {
        self.failure.is_some()
    }

    pub(super) fn check(&mut self, succeeded: bool) -> Result<(), FsError> {
        if let Some(failure) = self.failure.take() {
            return Err(failure);
        }
        if succeeded && !self.registered {
            return Err(error(FsErrorKind::IdentityUnavailable, self.target));
        }
        Ok(())
    }
}

fn opened_identity(handle: HANDLE) -> io::Result<Option<FileIdentity>> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::other("invalid CopyFile2 destination handle"));
    }
    // SAFETY: CopyFile2 keeps this non-null destination handle alive throughout the callback.
    let borrowed = unsafe { BorrowedHandle::borrow_raw(handle) };
    let duplicate = File::from(borrowed.try_clone_to_owned()?);
    identity::from_file(&duplicate)
}
