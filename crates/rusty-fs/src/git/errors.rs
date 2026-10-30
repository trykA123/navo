use std::io;

use rusty_core::{FsError, FsErrorKind, Location};
use rusty_git::GitError;

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

pub(super) fn git(error: GitError, location: &Location) -> FsError {
    let kind = match error {
        GitError::UnsafeDirectory { .. } => FsErrorKind::AccessDenied,
        GitError::NotARepository { .. } => FsErrorKind::NotFound,
        GitError::Unsupported { .. } => FsErrorKind::NotSupportedYet,
        GitError::Timeout => FsErrorKind::Timeout,
        GitError::MissingObject { .. } => FsErrorKind::NotFound,
        GitError::UnrepresentablePath => FsErrorKind::NameInvalid,
        GitError::GitNotFound | GitError::GitTooOld { .. } => FsErrorKind::NotSupportedYet,
        GitError::Io { kind, .. } => match kind {
            io::ErrorKind::NotFound => FsErrorKind::NotFound,
            io::ErrorKind::PermissionDenied => FsErrorKind::AccessDenied,
            io::ErrorKind::TimedOut => FsErrorKind::Timeout,
            _ => FsErrorKind::Unknown,
        },
        GitError::Command { stderr, .. } if not_a_repository(&stderr) => FsErrorKind::NotFound,
        _ => FsErrorKind::Unknown,
    };
    at(kind, location)
}

fn not_a_repository(stderr: &str) -> bool {
    stderr.contains("not a git repository")
        || stderr.contains("unknown revision")
        || stderr.contains("ambiguous argument")
}
