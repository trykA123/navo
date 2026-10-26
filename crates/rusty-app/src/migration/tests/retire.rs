use super::super::{Outcome, RealFs, Step};
use super::fault_fs::{Fault, FaultFs};
use super::faults::finishes_after_retry;
use super::fixture::{Fixture, layouts};

fn denied_retire(fixture: &Fixture) -> FaultFs {
    let name = if fixture.in_place() {
        "ferrite.db.navo-migrating"
    } else {
        "Ferrite"
    };
    FaultFs::new(Fault::Deny(name))
}

fn assert_navo_usable(fixture: &Fixture, original: &super::fixture::Tree) {
    let (old, new) = (fixture.data_rel(), fixture.new_data_rel());
    assert_eq!(
        fixture.read(&format!("{new}/navo.db")),
        original[&format!("{old}/ferrite.db")]
    );
    let (old, new) = (fixture.config_rel(), fixture.new_config_rel());
    assert_eq!(
        fixture.read(&format!("{new}/navo.toml")),
        original[&format!("{old}/ferrite.toml")]
    );
}

#[test]
fn a_retire_that_keeps_failing_lets_navo_start_and_retries_on_the_next_start() {
    for make in layouts() {
        let fixture = make();
        let original = fixture.tree();
        for _ in 0..2 {
            let outcome = fixture.run(&denied_retire(&fixture));
            assert!(
                matches!(&outcome, Outcome::MigratedRetirePending(Some(f)) if f.step == Step::Retire),
                "{outcome:?}"
            );
            assert_navo_usable(&fixture, &original);
        }
        finishes_after_retry(&fixture, &original, "retire allowed again");
    }
}

#[test]
fn a_running_ferrite_after_the_commit_does_not_block_navo() {
    for make in layouts() {
        let fixture = make();
        let original = fixture.tree();
        let _ = fixture.run(&denied_retire(&fixture));
        let before = fixture.tree();
        let outcome = fixture.migration(&RealFs).run(|| true);
        assert!(
            matches!(outcome, Outcome::MigratedRetirePending(None)),
            "{outcome:?}"
        );
        assert!(fixture.tree() == before);
        finishes_after_retry(&fixture, &original, "after Ferrite closed");
    }
}

#[cfg(windows)]
#[test]
fn access_denied_on_a_folder_rename_counts_as_in_use() {
    use super::super::Failure;
    let denied = |step| Failure::new(step, std::io::Error::from_raw_os_error(5), true);
    assert!(denied(Step::Retire).is_in_use());
    assert!(denied(Step::Commit).is_in_use());
    assert!(!denied(Step::Detach).is_in_use());
}
