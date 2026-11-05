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
        || Duration::ZERO,
        |_| panic!("a zero wait must not pause"),
    );
    assert_eq!(result.unwrap(), Some(42));
}

#[test]
fn errors_other_than_sharing_violations_fail_without_retry() {
    for code in [5, 2, 33] {
        let error = acquire_file::<()>(
            Duration::from_secs(60),
            || Err(io::Error::from_raw_os_error(code)),
            || Duration::ZERO,
            |_| panic!("only sharing violations may retry"),
        )
        .unwrap_err();
        assert_eq!(error.raw_os_error(), Some(code));
    }
}
