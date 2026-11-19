//! Journal-reset tests for `Rebuilds`: a lost journal versus verification, retries and the window.

use super::*;
use navo_index::table::TableError;

/// Runs a due reset before an overdue verification, then schedules verification from its result.
#[test]
fn a_due_journal_reset_runs_first_and_restarts_verification() {
    for quality in [Some(InexactBuild::HeldCap), None] {
        let shared = shared();
        let mut rebuilds = Rebuilds::new('C', Log::default());
        let (now, window) = (Instant::now(), REBUILD_WINDOW);
        let initial = Cause::new(BuildReason::SnapshotMissing);
        rebuilds.started(&shared, initial, Duration::ZERO);
        rebuilds.schedule_verification(now, "build_cycle");
        let lost = Err(NtfsError::Reenumerate("cursor"));
        assert_eq!(rebuilds.after_follow(&shared, lost, MINUTE).unwrap(), None);
        assert_eq!(rebuilds.verification_due(now + window), None);
        let reset = Cause::new(BuildReason::JournalReset);
        assert_eq!(rebuilds.due(window), Some(reset), "the reset wins");
        rebuilds.started(&shared, reset, window);
        rebuilds.build_quality(quality, now + window);
        rebuilds.regular_build_published();
        let mut due_after = |wait| rebuilds.verification_due(now + window + wait);
        let checks = [Duration::ZERO, 9 * MINUTE, 10 * MINUTE].map(&mut due_after);
        // Next tick, 9 min and 10 min after the reset published.
        assert_eq!(checks, [None, None, quality.and(Some("held_cap"))]);
    }
}

/// Holds an overdue verification while a journal reset waits, so one build serves both.
#[test]
fn a_waiting_journal_reset_holds_an_overdue_verification() {
    let shared = shared();
    let mut rebuilds = Rebuilds::new('C', Log::default());
    let now = Instant::now();
    rebuilds.started(
        &shared,
        Cause::new(BuildReason::SnapshotMissing),
        Duration::ZERO,
    );
    rebuilds.schedule_verification(now, "build_cycle");
    let lost = Err(NtfsError::Reenumerate("cursor"));
    assert_eq!(rebuilds.after_follow(&shared, lost, MINUTE).unwrap(), None);
    // Verification is overdue by its own clock; the reset is not due until the window ends.
    assert_eq!(rebuilds.verification_due(now + 20 * MINUTE), None);
    assert!(rebuilds.journal_lost());
}

/// Keeps a failed journal reset due after its regular build retry delay.
#[test]
fn a_failed_journal_reset_stays_pending_for_the_build_retry() {
    let shared = shared();
    let mut rebuilds = Rebuilds::new('C', Log::default());
    let initial = Cause::new(BuildReason::SnapshotMissing);
    rebuilds.started(&shared, initial, Duration::ZERO);
    let lost = Err(NtfsError::Reenumerate("cursor"));
    assert_eq!(rebuilds.after_follow(&shared, lost, MINUTE).unwrap(), None);
    let cause = rebuilds.due(REBUILD_WINDOW).unwrap();
    rebuilds.started(&shared, cause, REBUILD_WINDOW);
    assert!(rebuilds.build_result(enumerate_io_error()).is_err());

    let reset_pending = rebuilds.journal_lost();
    assert!(reset_pending, "a failed build keeps the reset pending");
    let backoff = rebuilds.take_retry_delay().unwrap();
    assert_eq!(backoff, Duration::from_secs(2));
    assert_eq!(rebuilds.due(REBUILD_WINDOW + backoff), Some(cause));
    assert!(rebuilds.journal_reset_due(REBUILD_WINDOW + backoff));
}

/// Logs a lost journal once and replaces the existing deferral within its original window.
#[test]
fn a_lost_journal_takes_priority_over_an_unresolved_batch() {
    let shared = shared();
    let (directory, log) = logged();
    let mut rebuilds = Rebuilds::new('C', log.clone());
    rebuilds.started(&shared, Cause::new(BuildReason::SnapshotMissing), MINUTE);
    let changes = create_under_unknown_folder(&shared, 120);
    assert_eq!(
        rebuilds
            .after_follow(&shared, Ok(changes), 5 * MINUTE)
            .unwrap(),
        None
    );
    assert!(!rebuilds.journal_lost());

    for minute in 6..=8 {
        let followed = Err(NtfsError::Reenumerate("cursor"));
        assert_eq!(
            rebuilds
                .after_follow(&shared, followed, minute * MINUTE)
                .unwrap(),
            None
        );
        // Heartbeats between attempts prevent the logger from collapsing identical lines.
        log.line("heartbeat");
    }

    assert_eq!(
        log_text(&directory)
            .matches("update error C: reenumerate(cursor)")
            .count(),
        1
    );
    assert!(rebuilds.journal_lost());
    assert_eq!(builds(&shared).deferred_rebuilds, 1);
    assert_eq!(
        rebuilds.due(11 * MINUTE),
        Some(Cause::new(BuildReason::JournalReset))
    );
}

#[test]
fn a_journal_reset_after_the_window_builds_at_once() {
    let shared = shared();
    let mut rebuilds = Rebuilds::new('C', Log::default());
    rebuilds.started(
        &shared,
        Cause::new(BuildReason::SnapshotMissing),
        Duration::ZERO,
    );
    let decision = rebuilds.after_follow(
        &shared,
        Err(NtfsError::Reenumerate("journal")),
        REBUILD_WINDOW,
    );
    assert_eq!(
        decision.unwrap(),
        Some(Cause::new(BuildReason::JournalReset))
    );
    assert_eq!(
        builds(&shared).last_reason,
        Some(BuildReason::SnapshotMissing)
    );
}

/// Logs the OS code or kind of a failed hard-link MFT read, never a name, and requests a reset.
#[test]
fn a_failed_hard_link_read_logs_its_os_code_but_no_name() {
    let shared = shared();
    let (directory, log) = logged();
    let mut rebuilds = Rebuilds::new('C', log);
    let read = |source| {
        let failed = NtfsError::Io {
            operation: "read MFT record",
            source,
        };
        Err(NtfsError::LinkRefresh(Box::new(failed)))
    };
    let coded = read(std::io::Error::from_raw_os_error(1117));
    let decision = rebuilds.after_follow(&shared, coded, MINUTE).unwrap();
    assert_eq!(decision, Some(Cause::new(BuildReason::JournalReset)));
    let named = read(std::io::Error::other("sentinel-secret.txt"));
    rebuilds.after_follow(&shared, named, MINUTE).unwrap();
    let text = log_text(&directory);
    let line = "update error C: reenumerate(hard link) io op=\"read MFT record\" os_error=1117";
    assert!(text.contains(line), "{text}");
    assert!(text.contains("kind=Other"), "{text}");
    assert!(!text.contains("sentinel-secret"), "{text}");
}

/// Pauses replay and saves after a failed apply, logs it once and runs one build after the window.
#[test]
fn a_failed_apply_pauses_replay_and_saves_until_one_rebuild_publishes() {
    let shared = shared();
    let (directory, log) = logged();
    let mut rebuilds = Rebuilds::new('C', log.clone());
    rebuilds.started(&shared, Cause::new(BuildReason::SnapshotMissing), MINUTE);
    let full = NtfsError::Table(TableError::Full);
    assert_eq!(rebuilds.apply_failed(&shared, &full, 2 * MINUTE), None);
    log.line("heartbeat");
    assert_eq!(rebuilds.apply_failed(&shared, &full, 3 * MINUTE), None);
    assert!(rebuilds.journal_lost(), "replay waits for the rebuild");
    assert!(
        rebuilds.is_deferred(),
        "the half-applied index is never saved"
    );
    let text = log_text(&directory);
    assert_eq!(
        text.matches("update error C: apply table").count(),
        1,
        "{text}"
    );
    let applied = Cause::new(BuildReason::ApplyFailed);
    let ready = MINUTE + REBUILD_WINDOW;
    assert!(rebuilds.journal_reset_due(ready));
    assert_eq!(rebuilds.due(ready), Some(applied));
    rebuilds.started(&shared, applied, ready);
    assert!(rebuilds.build_result(enumerate_io_error()).is_err());
    assert!(rebuilds.is_deferred(), "a failed build still blocks saves");
    assert_eq!(rebuilds.take_retry_delay(), Some(Duration::from_secs(2)));
    rebuilds.started(&shared, applied, ready + MINUTE);
    rebuilds.finished(&shared, 1, Duration::ZERO);
    rebuilds.regular_build_published();
    assert!(!rebuilds.journal_lost() && !rebuilds.is_deferred());
    assert_eq!(builds(&shared).last_reason, Some(BuildReason::ApplyFailed));
}

/// Builds at once after the window, and blocks saves before the build has started.
#[test]
fn a_failed_apply_after_the_window_builds_at_once_without_saving() {
    let shared = shared();
    let mut rebuilds = Rebuilds::new('C', Log::default());
    let full = NtfsError::Table(TableError::Full);
    let cause = rebuilds.apply_failed(&shared, &full, MINUTE);
    assert_eq!(cause, Some(Cause::new(BuildReason::ApplyFailed)));
    assert!(rebuilds.is_deferred() && rebuilds.journal_lost());
}
