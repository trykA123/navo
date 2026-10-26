use super::super::{Outcome, RealFs, Reason};
use super::fixture::{Fixture, Tree, crashed_where, layouts};

const FRESH: &[u8] = b"made by an old Ferrite";

fn crashed_after_detach(make: fn() -> Fixture) -> (Fixture, Tree) {
    crashed_where(make, |fixture| {
        let session = format!("{}/session.bin.navo-migrating", fixture.data_rel());
        fixture.journal_phase().as_deref() == Some("detached") && fixture.read(&session).is_some()
    })
}

fn original_bytes(fixture: &Fixture, original: &Tree, name: &str) -> Option<Vec<u8>> {
    original[&format!("{}/{name}", fixture.data_rel())].clone()
}

#[test]
fn try_again_after_an_old_ferrite_made_a_new_database_keeps_the_new_one_aside() {
    for make in layouts() {
        let (fixture, original) = crashed_after_detach(make);
        fixture.write(&format!("{}/ferrite.db", fixture.data_rel()), FRESH);
        let outcome = fixture.run(&RealFs);
        assert!(matches!(outcome, Outcome::Resumed), "{outcome:?}");
        let new = fixture.new_data_rel();
        assert_eq!(
            fixture.read(&format!("{new}/navo.db")),
            original_bytes(&fixture, &original, "ferrite.db")
        );
        assert_eq!(
            fixture.read(&format!("{new}/navo.db-wal")),
            original_bytes(&fixture, &original, "ferrite.db-wal")
        );
        let aside = format!(
            "{}/ferrite.db.created-during-migration",
            fixture.retired_data_rel()
        );
        assert_eq!(fixture.read(&aside).as_deref(), Some(FRESH));
    }
}

#[test]
fn start_fresh_after_an_old_ferrite_made_new_files_restores_the_real_set() {
    for make in layouts() {
        let (fixture, original) = crashed_after_detach(make);
        let data = fixture.data_rel();
        fixture.write(&format!("{data}/ferrite.db"), FRESH);
        fixture.write(&format!("{data}/session.bin"), FRESH);
        fixture.migration(&RealFs).start_fresh().unwrap();
        let created = |name: &str| format!("{data}/{name}.created-during-migration");
        assert_eq!(fixture.read(&created("ferrite.db")).as_deref(), Some(FRESH));
        assert_eq!(
            fixture.read(&created("session.bin")).as_deref(),
            Some(FRESH)
        );
        assert_eq!(
            fixture.read(&created("ferrite.db-shm")),
            original_bytes(&fixture, &original, "ferrite.db-shm")
        );
        let mut expected = original.clone();
        expected.remove(&format!("{data}/ferrite.db-shm"));
        let aside = [
            created("ferrite.db"),
            created("session.bin"),
            created("ferrite.db-shm"),
        ];
        let aside: Vec<&str> = aside.iter().map(String::as_str).collect();
        fixture.assert_started_fresh(&expected, &aside);
        assert!(matches!(
            fixture.run(&RealFs),
            Outcome::NothingToMigrate(Reason::StartedFresh)
        ));
    }
}

#[test]
fn a_taken_aside_name_moves_the_whole_set_to_the_next_free_number() {
    let (fixture, _) = crashed_after_detach(Fixture::user);
    let data = fixture.data_rel();
    fixture.write(&format!("{data}/ferrite.db"), FRESH);
    fixture.write(&format!("{data}/ferrite.db-wal"), FRESH);
    fixture.write(
        &format!("{data}/ferrite.db-wal.created-during-migration"),
        b"earlier",
    );
    fixture.migration(&RealFs).start_fresh().unwrap();
    let read = |name: &str| fixture.read(&format!("{data}/{name}"));
    assert_eq!(
        read("ferrite.db-wal.created-during-migration").as_deref(),
        Some(&b"earlier"[..])
    );
    assert_eq!(
        read("ferrite.db.created-during-migration-2").as_deref(),
        Some(FRESH)
    );
    assert_eq!(
        read("ferrite.db-wal.created-during-migration-2").as_deref(),
        Some(FRESH)
    );
    assert_eq!(read("ferrite.db-wal").as_deref(), Some(&b"wal frames"[..]));
}

fn staged_without_commit(fixture: &Fixture) -> bool {
    fixture.journal_phase().as_deref() == Some("staged") && !committed_any(fixture)
}

fn committed_any(fixture: &Fixture) -> bool {
    let config = fixture.new_config_rel();
    fixture.read(&format!("{config}/navo.toml")).is_some()
}

#[test]
fn start_fresh_at_staged_before_any_commit_rename_restores_the_old_files() {
    for make in layouts() {
        let (fixture, original) = crashed_where(make, staged_without_commit);
        fixture.migration(&RealFs).start_fresh().unwrap();
        fixture.assert_started_fresh(&original, &[]);
    }
}

#[test]
fn start_fresh_is_refused_after_the_first_commit_rename() {
    for make in layouts() {
        let (fixture, original) = crashed_where(make, |fixture| {
            fixture.journal_phase().as_deref() == Some("staged") && committed_any(fixture)
        });
        let before = fixture.tree();
        let failure = fixture.migration(&RealFs).start_fresh().unwrap_err();
        assert!(!failure.can_start_fresh());
        assert!(fixture.tree() == before);
        assert!(matches!(fixture.run(&RealFs), Outcome::Resumed));
        fixture.assert_one_navo_profile(&original);
    }
}
