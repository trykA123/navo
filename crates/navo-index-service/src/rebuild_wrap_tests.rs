//! Journal-wrap retry tests for `Rebuilds` (AF12): two continuity failures in a row let the third
//! regular build keep a wrapped enumeration, which schedules a `journal_wrapped` verification.

use navo_index::ntfs::Continuity;

use super::*;

/// The error a strict build returns when the journal no longer holds its start position.
fn wrapped() -> Result<(), NtfsError> {
    Err(NtfsError::Reenumerate("pass2_continuity"))
}

/// Records one successful regular build of two entries.
fn succeed(rebuilds: &mut Rebuilds, shared: &Shared) {
    rebuilds.started(
        shared,
        Cause::new(BuildReason::JournalReset),
        Duration::ZERO,
    );
    rebuilds.finished(shared, 2, Duration::from_millis(5));
}

#[test]
fn two_continuity_failures_let_the_third_regular_build_allow_a_wrap() {
    let mut rebuilds = Rebuilds::new('C', Log::default());
    let regular = Cause::new(BuildReason::JournalReset);
    assert_eq!(rebuilds.continuity(regular), Continuity::Strict);
    assert!(rebuilds.build_result(wrapped()).is_err());
    assert_eq!(rebuilds.continuity(regular), Continuity::Strict);
    assert!(rebuilds.build_result(wrapped()).is_err());
    assert_eq!(rebuilds.continuity(regular), Continuity::AllowWrap);
    assert_eq!(
        rebuilds.continuity(Cause::verification()),
        Continuity::Strict,
        "a verification checks the wrapped index strictly"
    );
}

#[test]
fn a_wrapped_build_schedules_a_journal_wrapped_verification_and_ends_the_streak() {
    let shared = shared();
    let (directory, log) = logged();
    let mut rebuilds = Rebuilds::new('C', log);
    let regular = Cause::new(BuildReason::JournalReset);
    rebuilds.build_result(wrapped()).unwrap_err();
    rebuilds.build_result(wrapped()).unwrap_err();
    let now = Instant::now();
    succeed(&mut rebuilds, &shared);
    rebuilds.build_quality(Some(InexactBuild::JournalWrapped), now);
    // The worker publishes the wrapped build like any regular build, ending the journal reset.
    rebuilds.regular_build_published();
    assert_eq!(rebuilds.continuity(regular), Continuity::Strict);
    assert_eq!(rebuilds.verification_due(now + 9 * MINUTE), None);
    assert_eq!(
        rebuilds.verification_due(now + 10 * MINUTE),
        Some("journal_wrapped")
    );
    let text = log_text(&directory);
    assert_eq!(
        text.matches("verify scheduled C: in 10 min (journal_wrapped)")
            .count(),
        1,
        "{text}"
    );
}

#[test]
fn a_successful_build_between_continuity_failures_restarts_the_streak() {
    let shared = shared();
    let mut rebuilds = Rebuilds::new('C', Log::default());
    rebuilds.build_result(wrapped()).unwrap_err();
    succeed(&mut rebuilds, &shared);
    rebuilds.build_result(wrapped()).unwrap_err();
    let regular = Cause::new(BuildReason::JournalReset);
    assert_eq!(rebuilds.continuity(regular), Continuity::Strict);
    rebuilds.build_result(wrapped()).unwrap_err();
    assert_eq!(rebuilds.continuity(regular), Continuity::AllowWrap);
}

#[test]
fn another_build_failure_restarts_the_streak() {
    let mut rebuilds = Rebuilds::new('C', Log::default());
    rebuilds.build_result(wrapped()).unwrap_err();
    rebuilds.build_result(enumerate_io_error()).unwrap_err();
    rebuilds.build_result(wrapped()).unwrap_err();
    let regular = Cause::new(BuildReason::JournalReset);
    assert_eq!(rebuilds.continuity(regular), Continuity::Strict);
}

#[test]
fn a_cancelled_build_keeps_the_streak() {
    let mut rebuilds = Rebuilds::new('C', Log::default());
    rebuilds.build_result(wrapped()).unwrap_err();
    rebuilds
        .build_result::<()>(Err(NtfsError::Cancelled))
        .unwrap_err();
    rebuilds.build_result(wrapped()).unwrap_err();
    let regular = Cause::new(BuildReason::JournalReset);
    assert_eq!(rebuilds.continuity(regular), Continuity::AllowWrap);
}

#[test]
fn a_cancelled_verification_keeps_the_wrapped_index_off_the_disk() {
    let shared = shared();
    let mut rebuilds = Rebuilds::new('C', Log::default());
    rebuilds.build_result(wrapped()).unwrap_err();
    rebuilds.build_result(wrapped()).unwrap_err();
    let now = Instant::now();
    succeed(&mut rebuilds, &shared);
    rebuilds.build_quality(Some(InexactBuild::JournalWrapped), now);
    rebuilds.regular_build_published();
    assert_eq!(rebuilds.save_cursor(), SaveCursor::Skip);
    let due = now + 10 * MINUTE;
    let reason = rebuilds.verification_due(due).unwrap();
    // The service stops while the strict verification runs.
    rebuilds
        .verification_result::<()>(Err(NtfsError::Cancelled), reason, due)
        .unwrap_err();
    assert_eq!(
        rebuilds.save_cursor(),
        SaveCursor::Skip,
        "a cancelled verification must leave the wrapped index unsaved"
    );
}
