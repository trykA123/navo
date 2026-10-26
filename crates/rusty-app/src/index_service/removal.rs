use std::io;

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

pub(super) fn remove_image(delete: impl FnMut() -> io::Result<()>) -> io::Result<()> {
    let mut delete = delete;
    super::install_plan::retry(5, std::time::Duration::from_millis(200), || {
        match delete() {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    })
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

    #[test]
    fn mapped_images_are_retried_before_cleanup_is_abandoned() {
        let attempts = Cell::new(0);
        remove_image(|| {
            attempts.set(attempts.get() + 1);
            if attempts.get() < 4 {
                Err(io::ErrorKind::PermissionDenied.into())
            } else {
                Ok(())
            }
        })
        .unwrap();
        assert_eq!(attempts.get(), 4);
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
    #[test]
    fn an_already_missing_image_needs_no_retry() {
        let mut attempts = 0;
        remove_image(|| {
            attempts += 1;
            Err(io::ErrorKind::NotFound.into())
        })
        .unwrap();
        assert_eq!(attempts, 1);
    }
}
