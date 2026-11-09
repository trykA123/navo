//! Remove step sequencing and the best-effort cleanup after the change lock drops.

use std::io;

mod files;
mod stop;

pub(super) use files::{LockedImage, removable_image};
pub(super) use stop::{INSTALL_STOP_LIMIT, PROCESS_WAIT};

#[cfg(windows)]
pub(super) use files::{CHANGE_LOCK, Cleanup, Entry, Files, Folders, Limits, clean, remove_parent};
#[cfg(windows)]
pub(super) use stop::{
    Control, POLL, REMOVE_DEADLINE, StartResult, State, Status, StopLimits, start, stop,
};

/// Runs the best-effort folder cleanup after the lock drops; the action result is unchanged.
pub(super) fn finish<T>(result: io::Result<T>, cleanup: impl FnOnce()) -> io::Result<T> {
    cleanup();
    result
}

pub(super) fn run(
    remove_service: impl FnOnce() -> io::Result<()>,
    cleanup: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    match remove_service() {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        result => result?,
    }
    cleanup()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn cleanup_never_changes_the_remove_result() {
        let cleaned = Cell::new(false);
        let result = finish(Ok(crate::Elevated::Done), || cleaned.set(true));
        assert!(cleaned.get());
        assert_eq!(result.unwrap(), crate::Elevated::Done);

        let error = finish::<crate::Elevated>(Err(io::ErrorKind::PermissionDenied.into()), || {});
        assert_eq!(error.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn removing_an_absent_service_still_cleans_its_files() {
        let cleaned = Cell::new(false);
        run(
            || Err(io::ErrorKind::NotFound.into()),
            || {
                cleaned.set(true);
                Ok(())
            },
        )
        .unwrap();
        assert!(cleaned.get());
    }
}

#[cfg(test)]
mod refusal_tests {
    use super::*;
    #[test]
    fn service_access_errors_do_not_authorize_file_cleanup() {
        let mut cleaned = false;
        assert!(
            run(
                || Err(io::ErrorKind::PermissionDenied.into()),
                || {
                    cleaned = true;
                    Ok(())
                }
            )
            .is_err()
        );
        assert!(!cleaned);
    }
}
