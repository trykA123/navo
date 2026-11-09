//! Exit code and outcome mapping between the app and an elevated service change.

use std::io;

use super::{Elevated, ElevationError};

const OTHER: u8 = 1;
const NOT_REPLACED: u8 = 3;
const TIMED_OUT: u8 = 4;
const DENIED: u8 = 5;
const LOCKED_INDEX: u8 = 7;
const LOCKED_NAVO: u8 = 8;
const LOCKED_FERRITE: u8 = 9;
/// Remove finished and Windows deletes its last files at the next restart.
pub(super) const DEFERRED: u8 = 6;

/// The exit code of an elevated run, so the app that asked can log why it failed.
pub(super) fn failure_code(error: &io::Error) -> u8 {
    if super::install_check::is_mismatch(error) {
        return NOT_REPLACED;
    }
    if let Some(locked) = error
        .get_ref()
        .and_then(|error| error.downcast_ref::<super::removal::LockedImage>())
    {
        match locked.path.file_name().and_then(|name| name.to_str()) {
            Some(name) if name.eq_ignore_ascii_case("navo-index.exe") => return LOCKED_INDEX,
            Some(name) if name.eq_ignore_ascii_case("navo.exe") => return LOCKED_NAVO,
            Some(name) if name.eq_ignore_ascii_case("ferrite.exe") => return LOCKED_FERRITE,
            _ => {}
        }
    }
    match error.kind() {
        io::ErrorKind::TimedOut => TIMED_OUT,
        io::ErrorKind::PermissionDenied => DENIED,
        _ => OTHER,
    }
}

/// The exit code of an elevated run that succeeded.
pub(super) fn success_code(outcome: Elevated) -> u8 {
    match outcome {
        Elevated::Done => 0,
        Elevated::Deferred => DEFERRED,
    }
}

/// Maps the elevated child's exit code back to its outcome or its reason.
pub(super) fn from_exit_code(code: u32) -> Result<Elevated, ElevationError> {
    if let Some((folder, image)) = locked_image(code) {
        #[cfg(windows)]
        let program_files =
            super::directory::known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_ProgramFiles);
        #[cfg(not(windows))]
        let program_files = Ok(std::path::PathBuf::from("Program Files"));
        let path = locked_path(program_files, folder, image);
        tracing::error!(path = %path.display(), "index service image is locked; close the program using it and retry");
        return Err(ElevationError::LockedImage(path));
    }
    match code {
        0 => Ok(Elevated::Done),
        code if code == u32::from(DEFERRED) => Ok(Elevated::Deferred),
        code => Err(ElevationError::Failed(code)),
    }
}

/// Builds the locked image path; when `Program Files` is unknown, keeps a relative path.
fn locked_path(
    program_files: io::Result<std::path::PathBuf>,
    folder: &str,
    image: &str,
) -> std::path::PathBuf {
    program_files
        .unwrap_or_else(|error| {
            tracing::debug!(%error, "cannot read Program Files; reporting a relative locked image path");
            std::path::PathBuf::from("Program Files")
        })
        .join(folder)
        .join("Index")
        .join(image)
}

fn locked_image(code: u32) -> Option<(&'static str, &'static str)> {
    use super::install_plan::{LEGACY_PRODUCT, NAVO};
    match u8::try_from(code).ok()? {
        LOCKED_INDEX => Some((NAVO.folder, NAVO.image)),
        LOCKED_NAVO => Some((NAVO.folder, "navo.exe")),
        LOCKED_FERRITE => Some((LEGACY_PRODUCT.folder, LEGACY_PRODUCT.image)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevated_child_exit_code_is_not_lost() {
        assert!(matches!(from_exit_code(0), Ok(Elevated::Done)));
        assert!(matches!(from_exit_code(1), Err(ElevationError::Failed(1))));
        assert!(matches!(
            from_exit_code(10),
            Err(ElevationError::Failed(10))
        ));
        assert!(!ElevationError::Cancelled.to_string().is_empty());
    }

    #[test]
    fn a_service_left_on_the_old_image_gets_its_own_exit_code_and_reason() {
        let mismatch = io::Error::other(super::super::install_check::Mismatch::OldProcess);
        let code = failure_code(&mismatch);
        assert_ne!(code, failure_code(&io::Error::other("anything else")));
        let text = ElevationError::Failed(u32::from(code)).to_string();
        assert!(text.contains("old one was kept"), "{text}");
    }

    #[test]
    fn exit_code_six_is_a_deferred_success_and_round_trips() {
        assert!(matches!(from_exit_code(6), Ok(Elevated::Deferred)));
        for outcome in [Elevated::Done, Elevated::Deferred] {
            let code = u32::from(success_code(outcome));
            assert_eq!(from_exit_code(code).unwrap(), outcome);
        }
        assert_ne!(DEFERRED, failure_code(&io::Error::other("anything")));
    }

    #[test]
    fn a_locked_image_path_stays_locked_when_program_files_is_unknown() {
        let path = locked_path(
            Err(io::Error::other("no known folder")),
            "Navo",
            "navo-index.exe",
        );
        assert!(path.is_relative(), "{}", path.display());
        assert!(
            path.ends_with("Navo/Index/navo-index.exe"),
            "{}",
            path.display()
        );
    }

    #[test]
    fn code_six_has_its_own_reason() {
        let text = ElevationError::Failed(6).to_string();
        assert!(text.contains("next restart"), "{text}");
    }
}
