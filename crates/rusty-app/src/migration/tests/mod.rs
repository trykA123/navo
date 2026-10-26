mod corrupt;
mod fault_fs;
mod faults;
mod fixture;
mod probe;
mod restore;
mod retire;
mod start;
mod sync;
mod wal;

use super::{Outcome, Phase, RealFs, Reason, test_exit_phase};
use fixture::Fixture;

#[test]
fn a_ferrite_profile_moves_to_navo_and_the_old_folders_are_renamed() {
    let fixture = Fixture::user();
    let original = fixture.tree();
    assert!(matches!(fixture.run(&RealFs), Outcome::Done));
    fixture.assert_one_navo_profile(&original);
}

#[test]
fn a_profile_folder_migrates_in_place() {
    let fixture = Fixture::profile();
    let original = fixture.tree();
    assert!(matches!(fixture.run(&RealFs), Outcome::Done));
    fixture.assert_one_navo_profile(&original);
}

#[test]
fn a_navo_folder_with_user_data_is_never_merged() {
    for name in ["navo.toml", "notes.txt"] {
        let fixture = Fixture::user();
        fixture.write(&format!("roaming/Navo/{name}"), b"mine");
        let before = fixture.tree();
        let outcome = fixture.run(&RealFs);
        assert!(
            matches!(
                outcome,
                Outcome::NothingToMigrate(Reason::NavoProfileExists)
            ),
            "{name}: {outcome:?}"
        );
        assert!(fixture.tree() == before);
    }
}

#[test]
fn a_profile_folder_that_already_has_navo_files_is_left_alone() {
    let fixture = Fixture::profile();
    fixture.write("p/data/navo.db", b"navo");
    let before = fixture.tree();
    let outcome = fixture.run(&RealFs);
    assert!(matches!(
        outcome,
        Outcome::NothingToMigrate(Reason::NavoProfileExists)
    ));
    assert!(fixture.tree() == before);
}

#[test]
fn an_empty_navo_folder_does_not_stop_the_migration() {
    let fixture = Fixture::user();
    let original = fixture.tree();
    std::fs::create_dir_all(fixture.path("local/Navo")).unwrap();
    assert!(matches!(fixture.run(&RealFs), Outcome::Done));
    fixture.assert_one_navo_profile(&original);
}

#[test]
fn the_completed_state_blocks_a_second_migration() {
    let fixture = Fixture::user();
    assert!(matches!(fixture.run(&RealFs), Outcome::Done));
    std::fs::remove_file(fixture.path("roaming/Navo/navo.toml")).unwrap();
    std::fs::remove_file(fixture.path("local/Navo/navo.db")).unwrap();
    fixture.write("roaming/Ferrite/ferrite.toml", b"old Ferrite ran again");
    fixture.write("local/Ferrite/ferrite.db", b"old Ferrite ran again");
    let before = fixture.tree();
    let outcome = fixture.run(&RealFs);
    assert!(matches!(
        outcome,
        Outcome::NothingToMigrate(Reason::AlreadyMigrated)
    ));
    assert!(fixture.tree() == before);
}

#[test]
fn a_running_ferrite_blocks_without_touching_anything() {
    let fixture = Fixture::user();
    let before = fixture.tree();
    let outcome = fixture.migration(&RealFs).run(|| true);
    assert!(matches!(outcome, Outcome::Blocked));
    assert!(fixture.tree() == before);
}

#[test]
fn without_a_ferrite_profile_nothing_is_created() {
    let fixture = Fixture::user();
    std::fs::remove_dir_all(fixture.path("roaming/Ferrite")).unwrap();
    std::fs::remove_dir_all(fixture.path("local/Ferrite")).unwrap();
    let before = fixture.tree();
    let outcome = fixture.migration(&RealFs).run(|| panic!("no probe needed"));
    assert!(matches!(
        outcome,
        Outcome::NothingToMigrate(Reason::NoLegacyProfile)
    ));
    assert!(fixture.tree() == before);
}

#[test]
fn a_taken_retired_name_gets_a_numeric_suffix() {
    let fixture = Fixture::user();
    fixture.write("roaming/Ferrite.migrated-to-navo/keep.txt", b"earlier");
    fixture.write("roaming/Ferrite.migrated-to-navo-2/keep.txt", b"earlier 2");
    assert!(matches!(fixture.run(&RealFs), Outcome::Done));
    let tree = fixture.tree();
    assert_eq!(
        tree["roaming/Ferrite.migrated-to-navo/keep.txt"].as_deref(),
        Some(&b"earlier"[..])
    );
    assert_eq!(
        tree["roaming/Ferrite.migrated-to-navo-2/keep.txt"].as_deref(),
        Some(&b"earlier 2"[..])
    );
    assert!(tree.contains_key("roaming/Ferrite.migrated-to-navo-3/ferrite.toml"));
    assert!(!tree.contains_key("roaming/Ferrite"));
}

#[test]
fn start_fresh_leaves_the_old_folders_untouched_and_never_migrates() {
    let fixture = Fixture::user();
    let before = fixture.tree();
    fixture.migration(&RealFs).start_fresh().unwrap();
    let mut after = fixture.tree();
    assert!(
        after
            .remove(&format!("local/Navo/{}", fixture::MARKER))
            .is_some()
    );
    assert_eq!(after.remove("local/Navo"), Some(None));
    assert!(after == before);
    let outcome = fixture.run(&RealFs);
    assert!(matches!(
        outcome,
        Outcome::NothingToMigrate(Reason::StartedFresh)
    ));
}

const REBUILDABLE: [&str; 5] = [
    "logs/navo.log",
    "index/walk-C.index",
    "cache.db",
    "running",
    "navo.lock",
];

fn add_rebuildable(fixture: &Fixture) {
    for rel in REBUILDABLE {
        fixture.write(&format!("local/Navo/{rel}"), b"navo");
    }
}

fn take_rebuildable(fixture: &Fixture) {
    for rel in REBUILDABLE {
        let path = fixture.path(&format!("local/Navo/{rel}"));
        assert_eq!(std::fs::read(&path).unwrap(), b"navo", "{rel} is kept");
        std::fs::remove_file(path).unwrap();
    }
    for dir in ["logs", "index"] {
        std::fs::remove_dir(fixture.path(&format!("local/Navo/{dir}"))).unwrap();
    }
}

#[test]
fn a_navo_folder_with_only_rebuildable_files_still_migrates() {
    let fixture = Fixture::user();
    let original = fixture.tree();
    add_rebuildable(&fixture);
    let outcome = fixture.run(&RealFs);
    assert!(matches!(outcome, Outcome::Done), "{outcome:?}");
    take_rebuildable(&fixture);
    fixture.assert_one_navo_profile(&original);
}

#[test]
fn a_navo_folder_that_gained_logs_while_staged_still_commits() {
    let (fixture, original) = fixture::crashed_where(Fixture::user, |fixture| {
        fixture.journal_phase().as_deref() == Some("staged")
            && fixture.read("roaming/Navo/navo.toml").is_none()
    });
    add_rebuildable(&fixture);
    let outcome = fixture.run(&RealFs);
    assert!(matches!(outcome, Outcome::Resumed), "{outcome:?}");
    take_rebuildable(&fixture);
    fixture.assert_one_navo_profile(&original);
}

#[test]
fn the_test_exit_hook_needs_the_crash_test_switch() {
    let vars = |crash: &'static str| {
        move |name: &str| match name {
            "NAVO_CRASH_TEST" => Some(crash.to_owned()),
            "NAVO_MIGRATION_TEST_EXIT" => Some("staged".to_owned()),
            _ => None,
        }
    };
    assert_eq!(test_exit_phase(vars("1")), Some(Phase::Staged));
    assert_eq!(test_exit_phase(vars("0")), None);
    assert_eq!(test_exit_phase(|_: &str| None), None);
}
