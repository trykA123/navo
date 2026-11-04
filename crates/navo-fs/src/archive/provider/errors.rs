use std::io;

use navo_core::{FsError, FsErrorKind, Location};

pub(super) fn at(kind: FsErrorKind, location: &Location) -> FsError {
    FsError {
        location: Some(location.clone()),
        ..FsError::new(kind)
    }
}

pub(super) fn read_only(location: &Location) -> FsError {
    at(FsErrorKind::NotSupportedYet, location)
}

pub(super) fn not_found(location: &Location) -> FsError {
    at(FsErrorKind::NotFound, location)
}

pub(super) fn unsupported(location: &Location) -> FsError {
    at(FsErrorKind::NotSupportedYet, location)
}

pub(super) fn name_invalid(location: &Location) -> FsError {
    at(FsErrorKind::NameInvalid, location)
}

pub(super) fn changed(location: &Location) -> FsError {
    at(FsErrorKind::ChangedOnDisk, location)
}

pub(super) fn cancelled(location: &Location) -> FsError {
    at(FsErrorKind::Cancelled, location)
}

pub(super) fn from_io(error: &io::Error, location: &Location) -> FsError {
    let kind = match error.kind() {
        io::ErrorKind::NotFound => FsErrorKind::NotFound,
        io::ErrorKind::PermissionDenied => FsErrorKind::AccessDenied,
        io::ErrorKind::TimedOut => FsErrorKind::Timeout,
        io::ErrorKind::InvalidInput | io::ErrorKind::IsADirectory => FsErrorKind::NotSupportedYet,
        _ => FsErrorKind::Unknown,
    };
    at(kind, location)
}
