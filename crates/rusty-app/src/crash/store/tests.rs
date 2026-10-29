use std::fs;
use std::path::{Path, PathBuf};

use rusty_fs::FakeFs;
use rusty_store::Db;

mod failures;
mod jobs;
mod retained;

use super::state::{Progress, State};
use super::*;

fn database(data: &Path) -> PathBuf {
    data.join("navo.db")
}

fn damaged(data: &Path) -> Vec<u8> {
    let path = database(data);
    super::super::test_damage::fill_history(&path);
    let (db, _) = Db::open(&path, false).unwrap();
    db.connection()
        .execute("INSERT INTO tags VALUES ('/sample', 'keep')", [])
        .unwrap();
    drop(db);
    super::super::test_damage::corrupt_middle_page(&path);
    fs::read(path).unwrap()
}

#[test]
fn damaged_storage_is_rebuilt_before_normal_database_opens() {
    let data = tempfile::tempdir().unwrap();
    let bytes = damaged(data.path());
    let mut start = check_with(Some(data.path()), true, &FakeFs::new(), true);
    assert!(start.notice.is_some());
    assert_eq!(
        fs::read(data.path().join("navo.db.corrupt-1")).unwrap(),
        bytes
    );
    let (db, report) = super::super::open_db(&database(data.path())).unwrap();
    assert!(!report.checked);
    assert_eq!(
        db.connection()
            .query_row("SELECT tag FROM tags", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "keep"
    );
    assert!(State::load(data.path()).unwrap().pending.is_none());
    start.share();
}

#[test]
fn a_database_connection_keeps_the_session_alive_after_startup_is_dropped() {
    let data = tempfile::tempdir().unwrap();
    let mut first = check_with(Some(data.path()), false, &FakeFs::new(), true);
    let (db, _) = super::super::open_db(&database(data.path())).unwrap();
    first.share();
    drop(first);
    let mut second = check_with(Some(data.path()), true, &FakeFs::new(), true);
    assert!(!second.locks.as_ref().unwrap().sole);
    second.share();
    drop(second);
    drop(db);
    let third = check_with(Some(data.path()), false, &FakeFs::new(), true);
    assert!(third.locks.as_ref().unwrap().sole);
}

#[test]
fn a_failed_preflight_blocks_opens_even_if_the_sentinel_cannot_be_written() {
    let data = tempfile::tempdir().unwrap();
    fs::write(State::path(data.path()), b"not a recovery record").unwrap();
    fs::create_dir(super::super::sentinel_path(&database(data.path()))).unwrap();
    let start = check_with(Some(data.path()), true, &FakeFs::new(), true);
    assert!(start.notice.is_some());
    let content = start.notice.as_ref().unwrap().content();
    assert!(
        content
            .details
            .last()
            .unwrap()
            .ends_with(&rusty_l10n::tr("retention-safe-mode-help"))
    );
    assert!(super::super::open_db(&database(data.path())).is_err());
    assert!(!database(data.path()).exists());
    assert_eq!(
        fs::read(State::path(data.path())).unwrap(),
        b"not a recovery record"
    );
}

#[test]
fn a_notice_survives_a_clean_restart_until_dismissed() {
    let data = tempfile::tempdir().unwrap();
    damaged(data.path());
    let mut first = check_with(Some(data.path()), true, &FakeFs::new(), true);
    first.share();
    drop(first);
    let mut second = check_with(Some(data.path()), false, &FakeFs::new(), true);
    assert!(second.notice.is_some());
    second.share();
    second.notice.as_ref().unwrap().dismiss().unwrap();
    drop(second);
    let third = check_with(Some(data.path()), false, &FakeFs::new(), true);
    assert!(third.notice.is_none());
}

#[test]
fn dismissal_preserves_older_cases_that_were_not_displayed() {
    let data = tempfile::tempdir().unwrap();
    damaged(data.path());
    let mut first = check_with(Some(data.path()), true, &FakeFs::new(), true);
    first.share();
    drop(first);
    let mut state = State::load(data.path()).unwrap();
    let sample = state.notices[0].clone();
    for case in 2..=4 {
        let mut notice = sample.clone();
        notice.case = case;
        state.notices.push(notice);
    }
    state.next_case = 5;
    state.save(data.path()).unwrap();
    let mut second = check_with(Some(data.path()), false, &FakeFs::new(), true);
    second.share();
    second.notice.as_ref().unwrap().dismiss().unwrap();
    drop(second);
    let state = State::load(data.path()).unwrap();
    assert_eq!(state.notices.len(), 1);
    assert_eq!(state.notices[0].case, 1);
    let third = check_with(Some(data.path()), false, &FakeFs::new(), true);
    assert!(third.notice.is_some());
}

#[test]
fn quarantine_completion_is_resumed_after_the_move_record_is_gone() {
    let data = tempfile::tempdir().unwrap();
    damaged(data.path());
    let mut state = State::default();
    state.begin(data.path()).unwrap();
    rusty_store::quarantine_database(&database(data.path())).unwrap();
    let start = check_with(Some(data.path()), false, &FakeFs::new(), true);
    assert!(start.notice.is_some());
    assert!(Db::open(&database(data.path()), true).is_ok());
    assert!(State::load(data.path()).unwrap().pending.is_none());
}

fn built(data: &Path) -> State {
    damaged(data);
    let mut state = State::default();
    state.begin(data).unwrap();
    let source = rusty_store::quarantine_database(&database(data))
        .unwrap()
        .unwrap();
    state.pending = Some(Progress::Build {
        case: 1,
        slot: source.slot,
        database: true,
    });
    state.save(data).unwrap();
    build::rebuild(data, &mut state, (1, source.slot, true)).unwrap();
    state
}

#[test]
fn promotion_resumes_its_own_hard_link_without_replacing_another_file() {
    let data = tempfile::tempdir().unwrap();
    let state = built(data.path());
    let Some(Progress::Promote { notice, .. }) = &state.pending else {
        panic!("built candidate");
    };
    let stage = state::candidate(data.path(), notice.case, notice.attempt);
    fs::hard_link(&stage, database(data.path())).unwrap();
    let start = check_with(Some(data.path()), false, &FakeFs::new(), true);
    assert!(start.notice.is_some());
    assert!(!stage.exists());
    assert!(State::load(data.path()).unwrap().pending.is_none());
}

#[test]
fn promotion_refuses_a_replacement_database_even_with_matching_bytes() {
    let data = tempfile::tempdir().unwrap();
    let state = built(data.path());
    let Some(Progress::Promote { notice, .. }) = &state.pending else {
        panic!("built candidate");
    };
    let stage = state::candidate(data.path(), notice.case, notice.attempt);
    let bytes = fs::read(&stage).unwrap();
    fs::write(database(data.path()), &bytes).unwrap();
    let start = check_with(Some(data.path()), false, &FakeFs::new(), true);
    assert_eq!(
        start.notice.as_ref().unwrap().content().title,
        rusty_l10n::tr("store-recovery-blocked")
    );
    assert_eq!(fs::read(database(data.path())).unwrap(), bytes);
    assert!(stage.exists());
    assert!(super::super::open_db(&database(data.path())).is_err());
}

#[test]
fn an_interrupted_build_keeps_the_old_attempt_and_uses_an_empty_candidate() {
    let data = tempfile::tempdir().unwrap();
    damaged(data.path());
    let mut state = State::default();
    state.begin(data.path()).unwrap();
    let source = rusty_store::quarantine_database(&database(data.path()))
        .unwrap()
        .unwrap();
    state.pending = Some(Progress::Build {
        case: 1,
        slot: source.slot,
        database: true,
    });
    state.save(data.path()).unwrap();
    let abandoned = state::candidate(data.path(), 1, 1);
    fs::write(&abandoned, b"partial previous attempt").unwrap();
    let start = check_with(Some(data.path()), false, &FakeFs::new(), true);
    assert!(start.notice.is_some());
    assert_eq!(fs::read(abandoned).unwrap(), b"partial previous attempt");
    assert_eq!(State::load(data.path()).unwrap().notices[0].attempt, 2);
}

#[test]
fn the_completed_receipt_is_acknowledged_on_the_next_start() {
    let data = tempfile::tempdir().unwrap();
    built(data.path());
    workflow::run(
        data.path(),
        &workflow::Mode {
            unclean: false,
            experimental: false,
            sole: true,
        },
        &FakeFs::new(),
    )
    .unwrap();
    let notice = State::load(data.path()).unwrap().notices[0].clone();
    fs::write(
        files::suffix(&database(data.path()), ".quarantine-receipt"),
        format!("{} database\n", notice.slot),
    )
    .unwrap();
    let start = check_with(Some(data.path()), false, &FakeFs::new(), true);
    assert!(start.notice.is_some());
    assert!(!files::suffix(&database(data.path()), ".quarantine-receipt").exists());
}

#[test]
fn promotion_identity_domains_cannot_match_through_identical_numeric_fields() {
    let data = tempfile::tempdir().unwrap();
    let state = built(data.path());
    let Some(Progress::Promote {
        notice,
        mut identity,
    }) = state.pending
    else {
        panic!("built candidate");
    };
    let stage = state::candidate(data.path(), notice.case, notice.attempt);
    let actual = identity.kind;
    for kind in [
        rusty_core::FileIdentityKind::Native,
        rusty_core::FileIdentityKind::WindowsIndex,
        rusty_core::FileIdentityKind::Fingerprint { folder: false },
        rusty_core::FileIdentityKind::Fingerprint { folder: true },
    ] {
        identity.kind = kind;
        assert_eq!(promote::matches(&stage, &identity).unwrap(), kind == actual);
    }
}

#[test]
fn native_promotion_identity_records_remain_compatible_and_legacy_domains_round_trip() {
    let text = r#"{"volume":1,"file":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1]}"#;
    let mut identity: state::Identity = serde_json::from_str(text).unwrap();
    assert_eq!(identity.kind, rusty_core::FileIdentityKind::Native);
    assert_eq!(
        serde_json::to_value(&identity).unwrap(),
        serde_json::from_str::<serde_json::Value>(text).unwrap()
    );
    identity.kind = rusty_core::FileIdentityKind::WindowsIndex;
    let restored: state::Identity =
        serde_json::from_slice(&serde_json::to_vec(&identity).unwrap()).unwrap();
    assert_eq!(restored.kind, rusty_core::FileIdentityKind::WindowsIndex);
}

#[test]
#[ignore = "requires FERRITE_QUARANTINE_TEST_ROOT on a native filesystem without hard links and with stable identities"]
fn complete_recovery_supports_a_native_filesystem_without_hard_links() {
    let root =
        std::env::var_os("FERRITE_QUARANTINE_TEST_ROOT").expect("select an isolated test root");
    let data = tempfile::tempdir_in(root).unwrap();
    let original = damaged(data.path());
    assert!(
        fs::hard_link(database(data.path()), data.path().join("link-probe")).is_err(),
        "the selected filesystem supports hard links"
    );
    let mut start = check_with(Some(data.path()), true, &FakeFs::new(), true);
    assert!(start.notice.is_some());
    let (db, _) = super::super::open_db(&database(data.path())).unwrap();
    assert_eq!(
        db.connection()
            .query_row("SELECT tag FROM tags", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "keep"
    );
    assert_eq!(
        fs::read(data.path().join("navo.db.corrupt-1")).unwrap(),
        original
    );
    assert!(State::load(data.path()).unwrap().pending.is_none());
    start.share();
}

fn replaced_stage_after_publication() -> (tempfile::TempDir, PathBuf, PathBuf, state::Identity) {
    let data = tempfile::tempdir().unwrap();
    let state = built(data.path());
    let Some(Progress::Promote { notice, identity }) = state.pending else {
        panic!("built candidate");
    };
    let stage = state::candidate(data.path(), notice.case, notice.attempt);
    let target = database(data.path());
    rusty_fs::native_rename_atomic(&stage, &target).unwrap();
    fs::write(&stage, b"replacement stage created after publication").unwrap();
    (data, stage, target, identity)
}

#[test]
fn a_consumed_candidate_path_is_never_cleaned_after_native_publication() {
    let (_data, stage, target, identity) = replaced_stage_after_publication();
    promote::finish((&stage, &target), &identity, true).unwrap();
    assert_eq!(
        fs::read(stage).unwrap(),
        b"replacement stage created after publication"
    );
    assert!(promote::matches(&target, &identity).unwrap());
}

#[test]
fn resumed_candidate_cleanup_preserves_an_unverified_replacement() {
    let (_data, stage, target, identity) = replaced_stage_after_publication();
    assert!(promote::finish((&stage, &target), &identity, false).is_err());
    assert_eq!(
        fs::read(stage).unwrap(),
        b"replacement stage created after publication"
    );
    assert!(promote::matches(&target, &identity).unwrap());
}

#[test]
fn default_startup_recovery_ignores_another_profiles_mutation_lease() {
    let data = tempfile::tempdir().unwrap();
    let exclusive = crate::jobs::test_mutation_file(data.path());
    exclusive.try_lock().unwrap();
    let start = check_with(Some(data.path()), false, &FakeFs::new(), false);
    assert!(start.notice.is_none());
    let (db, _) = super::super::open_db(&database(data.path())).unwrap();
    drop(db);
}

#[test]
fn main_compat_default_startup_preserves_a_bad_database_without_quarantine() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("navo.db");
    let bytes = b"not a SQLite database";
    std::fs::write(&path, bytes).unwrap();
    let start = check_with(Some(data.path()), true, &FakeFs::new(), false);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert!(start.notice.is_none());
    assert!(!data.path().join("navo.db.corrupt-1").exists());
}

#[test]
fn legacy_startup_keeps_middle_page_corruption_blocked_without_quarantine() {
    let data = tempfile::tempdir().unwrap();
    let bytes = damaged(data.path());
    let start = check_with(Some(data.path()), true, &FakeFs::new(), false);
    assert!(super::super::open_db(&database(data.path())).is_err());
    assert!(super::super::sentinel_path(&database(data.path())).exists());
    assert_eq!(fs::read(database(data.path())).unwrap(), bytes);
    assert!(start.notice.is_none());
}

#[test]
fn legacy_startup_rechecks_a_repaired_database_and_clears_its_sentinel() {
    let data = tempfile::tempdir().unwrap();
    let path = database(data.path());
    drop(Db::open(&path, false).unwrap());
    super::super::record_failed_check(&path);
    let start = check_with(Some(data.path()), false, &FakeFs::new(), false);
    assert!(super::super::open_db(&path).is_ok());
    assert!(!super::super::sentinel_path(&path).exists());
    assert!(start.notice.is_none());
}

#[test]
fn legacy_startup_prunes_finished_history_through_the_production_path() {
    let data = tempfile::tempdir().unwrap();
    let journal = crate::jobs::open(&database(data.path())).unwrap();
    for now in 0..55 {
        let job = crate::jobs::lock(&journal)
            .create_job("copy", b"{}", 0, now)
            .unwrap();
        crate::jobs::lock(&journal)
            .finish(job, rusty_store::JobState::Done)
            .unwrap();
    }
    drop(journal);
    let _start = check_with(Some(data.path()), false, &FakeFs::new(), false);
    let (db, _) = super::super::open_db(&database(data.path())).unwrap();
    let count: u32 = db
        .connection()
        .query_row("SELECT COUNT(*) FROM job", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, crate::jobs::KEEP_FINISHED);
}

#[test]
fn legacy_startup_records_corruption_detected_by_job_recovery_on_a_clean_start() {
    let data = tempfile::tempdir().unwrap();
    let path = database(data.path());
    let journal = crate::jobs::open(&path).unwrap();
    crate::jobs::lock(&journal)
        .create_job("copy", b"{}", 1, 0)
        .unwrap();
    drop(journal);
    let page = super::super::test_damage::root_page(&path, "job_step");
    super::super::test_damage::corrupt_page(&path, page);
    assert!(Db::open(&path, false).is_ok());
    let _start = check_with(Some(data.path()), false, &FakeFs::new(), false);
    assert!(super::super::sentinel_path(&path).exists());
    assert!(super::super::open_db(&path).is_err());
}
