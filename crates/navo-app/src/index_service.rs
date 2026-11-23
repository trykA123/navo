//! App-side facade over the index service library.

pub(crate) use navo_index_service::Action;

/// The SHA-256 of `navo-index.exe` that the release build passes in `NAVO_INDEX_SHA256` after it
/// built that image; empty in a developer build.
const BUILT_IN_SHA256: &str = match option_env!("NAVO_INDEX_SHA256") {
    Some(hash) => hash,
    None => "",
};

// A malformed value would refuse every install, so the build stops instead.
const _: () = assert!(
    is_sha256_or_empty(BUILT_IN_SHA256),
    "the NAVO_INDEX_SHA256 value must be 64 hex digits or empty"
);

/// True for an empty text or exactly 64 ASCII hex digits; evaluated at compile time.
const fn is_sha256_or_empty(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return true;
    }
    if bytes.len() != 64 {
        return false;
    }
    let mut index = 0;
    while index < bytes.len() {
        if !bytes[index].is_ascii_hexdigit() {
            return false;
        }
        index += 1;
    }
    true
}

/// The hash lookup the elevated install uses: the built-in value, or nothing in a tester build.
struct BuiltIn;

impl navo_index_service::ImageHash for BuiltIn {
    fn expected_sha256(&self) -> &str {
        if cfg!(feature = "tester") {
            ""
        } else {
            BUILT_IN_SHA256
        }
    }
}

/// Runs one index service action; an install refuses an image that is not the built one.
pub(crate) fn run(action: Action) -> std::process::ExitCode {
    navo_index_service::run(action, Some(&BuiltIn))
}

#[cfg(any(windows, test))]
pub(crate) use crate::service_version_step::is_older;
#[cfg(windows)]
pub(crate) use crate::service_version_step::start_version_step;
#[cfg(any(windows, test))]
pub(crate) use navo_index_service::{Elevated, ElevationError, ServiceQuery};
#[cfg(test)]
pub(crate) use navo_index_service::{ServiceDrive, selected_roots};
#[cfg(windows)]
pub(crate) use navo_index_service::{
    dismiss_upgrade_notice, installed_volume_roots, query, request_elevation, upgrade_notice_due,
};
