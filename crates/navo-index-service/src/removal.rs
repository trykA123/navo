use std::io;

mod files;
mod stop;

pub(super) use files::{LockedImage, removable_image};
pub(super) use stop::{PROCESS_WAIT, STOP_LIMIT};

#[cfg(windows)]
pub(super) use files::{Cleanup, Entry, Files, Folders, Limits, clean};
#[cfg(windows)]
pub(super) use stop::{Control, POLL, REMOVE_DEADLINE, State, Status, StopLimits, stop};

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
