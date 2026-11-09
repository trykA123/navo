//! Retry and deadline tests for acquiring the change lock file.

use std::cell::Cell;

use super::*;

#[test]
fn a_sharing_violation_retries_until_the_file_opens() {
    let elapsed = Cell::new(Duration::ZERO);
    let mut attempts = 0;
    let result = acquire_file(
        Duration::from_secs(1),
        || {
            attempts += 1;
            if attempts < 3 {
                Err(io::Error::from_raw_os_error(SHARING_VIOLATION))
            } else {
                Ok(42)
            }
        },
        retryable_lock_error,
        || elapsed.get(),
        |delay| elapsed.set(elapsed.get() + delay),
    );
    assert_eq!(result.unwrap(), Some(42));
    assert_eq!(attempts, 3);
}

#[test]
fn the_retry_wait_never_exceeds_the_deadline() {
    for wait in [
        Duration::ZERO,
        FILE_POLL - Duration::from_millis(1),
        FILE_POLL,
        FILE_POLL + Duration::from_millis(1),
    ] {
        let elapsed = Cell::new(Duration::ZERO);
        let mut attempts = 0;
        let result = acquire_file::<()>(
            wait,
            || {
                attempts += 1;
                Err(io::Error::from_raw_os_error(SHARING_VIOLATION))
            },
            retryable_lock_error,
            || elapsed.get(),
            |delay| elapsed.set(elapsed.get() + delay),
        );
        assert!(result.unwrap().is_none());
        assert_eq!(elapsed.get(), wait);
        assert_eq!(attempts, if wait > FILE_POLL { 2 } else { 1 });
    }
}

#[test]
fn zero_wait_still_opens_an_available_file_once() {
    let result = acquire_file(
        Duration::ZERO,
        || Ok(42),
        retryable_lock_error,
        || Duration::ZERO,
        |_| panic!("a zero wait must not pause"),
    );
    assert_eq!(result.unwrap(), Some(42));
}

#[test]
fn a_vanished_parent_is_recreated_and_the_lock_retries() {
    let mut attempts = 0;
    let result = acquire_file(
        Duration::from_secs(1),
        || {
            attempts += 1;
            if attempts == 1 {
                Err(io::Error::from_raw_os_error(3))
            } else {
                Ok(42)
            }
        },
        retryable_lock_error,
        || Duration::ZERO,
        |_| {},
    );
    assert_eq!(result.unwrap(), Some(42));
    assert_eq!(attempts, 2);
}

#[test]
fn a_delete_pending_or_held_parent_retries() {
    for code in [2, 3, 5, 32, 33] {
        let mut attempts = 0;
        let result = acquire_file(
            Duration::from_secs(1),
            || {
                attempts += 1;
                if attempts == 1 {
                    Err(io::Error::from_raw_os_error(code))
                } else {
                    Ok(42)
                }
            },
            retryable_lock_error,
            || Duration::ZERO,
            |_| {},
        );
        assert_eq!(result.unwrap(), Some(42), "code {code}");
        assert_eq!(attempts, 2, "code {code}");
    }
}

#[test]
fn lock_errors_outside_the_retry_set_fail_at_once() {
    for code in [1, 4, 6, 87] {
        let error = acquire_file::<()>(
            Duration::from_secs(60),
            || Err(io::Error::from_raw_os_error(code)),
            retryable_lock_error,
            || Duration::ZERO,
            |_| panic!("only the decided lock errors may retry"),
        )
        .unwrap_err();
        assert_eq!(error.raw_os_error(), Some(code));
    }
}
