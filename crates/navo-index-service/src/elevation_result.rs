use std::io;

use super::ElevationError;

const OTHER: u8 = 1;
const NOT_REPLACED: u8 = 3;
const TIMED_OUT: u8 = 4;
const DENIED: u8 = 5;

/// The exit code of an elevated run, so the app that asked can log why it failed.
pub(super) fn failure_code(error: &io::Error) -> u8 {
    if super::install_check::is_mismatch(error) {
        return NOT_REPLACED;
    }
    match error.kind() {
        io::ErrorKind::TimedOut => TIMED_OUT,
        io::ErrorKind::PermissionDenied => DENIED,
        _ => OTHER,
    }
}

pub(super) fn from_exit_code(code: u32) -> Result<(), ElevationError> {
    match code {
        0 => Ok(()),
        code => Err(ElevationError::Failed(code)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevated_child_exit_code_is_not_lost() {
        assert!(from_exit_code(0).is_ok());
        assert!(matches!(from_exit_code(7), Err(ElevationError::Failed(7))));
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
}
