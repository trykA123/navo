//! Conversions shared by the local and Git sides.

use navo_compare::model::{Reason, RelPath};
use navo_core::{FsErrorKind, ProviderPath};

pub fn reason_of(kind: FsErrorKind) -> Reason {
    match kind {
        FsErrorKind::NotFound => Reason::NotFound,
        FsErrorKind::Locked => Reason::Locked,
        FsErrorKind::AccessDenied | FsErrorKind::SharingViolation => Reason::AccessDenied,
        FsErrorKind::Cancelled => Reason::Stopped,
        FsErrorKind::NotSupportedYet | FsErrorKind::SourceFilterFailed => Reason::Unsupported,
        FsErrorKind::ChangedOnDisk => Reason::ChangedOnDisk,
        FsErrorKind::MissingPromisorBlob => Reason::MissingBlob,
        _ => Reason::ReadError,
    }
}

/// Joins a folder row path and a child name with `/`.
pub fn child_rel(parent: &RelPath, name: &ProviderPath) -> RelPath {
    let parent = parent.as_path().to_text();
    let name = name.to_text();
    let text = if parent.is_empty() {
        name
    } else {
        format!("{parent}/{name}")
    };
    RelPath::new(ProviderPath::from_text(&text))
}

/// The path segments of a row path, without empty parts.
pub fn segments(rel: &RelPath) -> Vec<String> {
    rel.as_path()
        .to_text()
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}
