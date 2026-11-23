//! Log-safe names for index and snapshot errors: static text and OS codes, never names or paths.

use navo_index::file::FileError;
use navo_index::ntfs::NtfsError;

/// A log-safe error kind: static text and OS codes, never names.
pub(crate) fn error_kind(error: &NtfsError) -> String {
    match error {
        NtfsError::Io { operation, source } => format!("io op=\"{operation}\" {}", io_kind(source)),
        NtfsError::InvalidVolume => "invalid_volume".into(),
        NtfsError::InvalidResponse(what) => format!("invalid_response \"{what}\""),
        NtfsError::Snapshot(error) => file_error_kind(error),
        NtfsError::Parse(_) => "parse".into(),
        NtfsError::Table(_) => "table".into(),
        NtfsError::Reenumerate(cause) => format!("reenumerate({cause})"),
        NtfsError::LinkRefresh(failed) => format!("reenumerate(hard link) {}", error_kind(failed)),
        NtfsError::FileGone => "file_gone".into(),
        NtfsError::Cancelled => "cancelled".into(),
    }
}

/// A log-safe snapshot file error kind: static text and OS codes, never paths.
pub(crate) fn file_error_kind(error: &FileError) -> String {
    match error {
        FileError::Io(source) => format!("snapshot_io {}", io_kind(source)),
        FileError::Allocation(_) => "snapshot_allocation".into(),
        FileError::BadMagic => "snapshot_bad_magic".into(),
        FileError::UnsupportedVersion { found, .. } => format!("snapshot_version found={found}"),
        FileError::BadLength => "snapshot_bad_length".into(),
        FileError::BadChecksum => "snapshot_bad_checksum".into(),
        FileError::InvalidData(what) => format!("snapshot_invalid \"{what}\""),
    }
}

/// Names an I/O error by its OS code, or by its kind when it has none.
fn io_kind(error: &std::io::Error) -> String {
    match error.raw_os_error() {
        Some(code) => format!("os_error={code}"),
        None => format!("kind={:?}", error.kind()),
    }
}
