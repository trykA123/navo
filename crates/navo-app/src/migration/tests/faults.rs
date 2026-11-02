use std::collections::HashSet;

use super::super::{Outcome, RealFs, Reason, Step};
use super::fault_fs::{Fault, FaultFs};
use super::fixture::{Fixture, Tree, layouts, write_count};

pub(super) fn finishes_after_retry(fixture: &Fixture, original: &Tree, label: &str) {
    let outcome = fixture.run(&RealFs);
    assert!(
        matches!(
            outcome,
            Outcome::Done | Outcome::Resumed | Outcome::NothingToMigrate(Reason::AlreadyMigrated)
        ),
        "{label}: retry gave {outcome:?}"
    );
    fixture.assert_one_navo_profile(original);
    assert!(
        matches!(fixture.run(&RealFs), Outcome::NothingToMigrate(_)),
        "{label}: a third run must not migrate"
    );
}

#[test]
fn every_write_counts_as_a_fault_point() {
    for make in layouts() {
        assert!(write_count(make) > 20);
    }
}

#[test]
fn a_fault_at_any_write_fails_cleanly_and_try_again_finishes() {
    for make in layouts() {
        for k in 0..write_count(make) {
            let fixture = make();
            let original = fixture.tree();
            let outcome = fixture.run(&FaultFs::new(Fault::FailAt(k)));
            let label = format!("fault at write {k}, in place {}", fixture.in_place());
            match &outcome {
                Outcome::Failed(failure)
                    if failure.can_start_fresh() && failure.step != Step::Commit =>
                {
                    assert!(fixture.tree() == original, "{label}: old files changed");
                }
                Outcome::Failed(_) | Outcome::MigratedRetirePending(Some(_)) => {}
                other => panic!("{label}: expected a failure, got {other:?}"),
            }
            finishes_after_retry(&fixture, &original, &label);
        }
    }
}

#[test]
fn a_process_crash_after_any_write_resumes_to_exactly_one_profile() {
    for make in layouts() {
        for k in 0..write_count(make) {
            let fixture = make();
            let original = fixture.tree();
            let outcome = fixture.run(&FaultFs::new(Fault::CrashAt(k)));
            assert!(
                matches!(
                    outcome,
                    Outcome::Failed(_) | Outcome::MigratedRetirePending(Some(_))
                ),
                "crash at {k}: {outcome:?}"
            );
            let label = format!("crash at write {k}, in place {}", fixture.in_place());
            finishes_after_retry(&fixture, &original, &label);
        }
    }
}

#[test]
fn start_fresh_after_a_process_crash_at_any_write_restores_or_is_refused() {
    for make in layouts() {
        let mut restored = 0;
        for k in 0..write_count(make) {
            let fixture = make();
            let original = fixture.tree();
            let label = format!("crash at write {k}, in place {}", fixture.in_place());
            let allowed = match fixture.run(&FaultFs::new(Fault::CrashAt(k))) {
                Outcome::Failed(failure) => failure.can_start_fresh(),
                Outcome::MigratedRetirePending(_) => false,
                other => panic!("{label}: {other:?}"),
            };
            let before = fixture.tree();
            let result = fixture.migration(&RealFs).start_fresh();
            assert_eq!(result.is_ok(), allowed, "{label}: {result:?}");
            if allowed {
                fixture.assert_started_fresh(&original, &[]);
                assert!(matches!(
                    fixture.run(&RealFs),
                    Outcome::NothingToMigrate(Reason::StartedFresh)
                ));
                restored += 1;
            } else {
                assert!(fixture.tree() == before, "{label}: a refusal changed files");
                finishes_after_retry(&fixture, &original, &label);
            }
        }
        assert!(restored > 5, "too few crash points before commit");
    }
}

type Seen = HashSet<(Tree, Option<bool>)>;

fn recovers_from_two_faults(make: fn() -> Fixture, k: usize, j: usize, seen: &mut Seen) -> bool {
    let label = format!("faults at {k} and {j}");
    let fixture = make();
    let original = fixture.tree();
    let fs = FaultFs::new(Fault::FailTwice(k, j));
    let can_start_fresh = match fixture.run(&fs) {
        Outcome::Failed(failure) => Some(failure.can_start_fresh()),
        _ => None,
    };
    if seen.insert((fixture.state(), can_start_fresh)) {
        finishes_after_retry(&fixture, &original, &label);
        if let Some(allowed) = can_start_fresh {
            starts_fresh_after_two_faults(make, k, j, allowed);
        }
    }
    fs.writes() > j
}

fn starts_fresh_after_two_faults(make: fn() -> Fixture, k: usize, j: usize, allowed: bool) {
    let fixture = make();
    let original = fixture.tree();
    let _ = fixture.run(&FaultFs::new(Fault::FailTwice(k, j)));
    let result = fixture.migration(&RealFs).start_fresh();
    assert_eq!(result.is_ok(), allowed, "faults at {k} and {j}");
    if result.is_ok() {
        fixture.assert_started_fresh(&original, &[]);
    }
}

#[test]
fn a_fault_in_the_rollback_or_unstage_still_recovers() {
    for make in layouts() {
        let mut seen = Seen::new();
        let count = write_count(make);
        for k in 0..count {
            let mut j = k + 1;
            while j < 2 * count && recovers_from_two_faults(make, k, j, &mut seen) {
                j += 1;
            }
        }
        assert!(seen.len() > 20, "too few distinct states: {}", seen.len());
    }
}

fn second_faults_from(make: fn() -> Fixture, k: usize, original: &Tree, seen: &mut HashSet<Tree>) {
    for j in 0..2 * write_count(make) {
        let fixture = make();
        let _ = fixture.run(&FaultFs::new(Fault::FailAt(k)));
        let fs = FaultFs::new(Fault::FailAt(j));
        let _ = fixture.run(&fs);
        if seen.insert(fixture.state()) {
            finishes_after_retry(&fixture, original, &format!("faults at {k} then {j}"));
        }
        if fs.writes() <= j {
            return;
        }
    }
}

#[test]
fn a_second_fault_during_try_again_still_finishes() {
    for make in layouts() {
        let original = make().tree();
        let (mut first, mut second) = (HashSet::new(), HashSet::new());
        for k in 0..write_count(make) {
            let fixture = make();
            let _ = fixture.run(&FaultFs::new(Fault::FailAt(k)));
            if first.insert(fixture.state()) {
                second_faults_from(make, k, &original, &mut second);
            }
        }
        assert!(
            first.len() > 10,
            "too few states after one fault: {}",
            first.len()
        );
    }
}

#[test]
fn a_copy_that_differs_from_its_source_fails_and_rolls_back() {
    for make in layouts() {
        let fixture = make();
        let original = fixture.tree();
        let outcome = fixture.run(&FaultFs::new(Fault::CorruptCopies));
        let Outcome::Failed(failure) = outcome else {
            panic!("a corrupt copy must fail, got {outcome:?}");
        };
        assert!(failure.can_start_fresh());
        assert_eq!(failure.error.kind(), std::io::ErrorKind::InvalidData);
        assert!(fixture.tree() == original);
    }
}

#[cfg(windows)]
#[test]
fn a_held_old_database_blocks_and_restores_the_detached_files() {
    let fixture = Fixture::user();
    let original = fixture.tree();
    let outcome = fixture.run(&FaultFs::new(Fault::InUse));
    assert!(matches!(outcome, Outcome::Blocked), "{outcome:?}");
    assert!(fixture.tree() == original);
}

#[test]
fn a_process_crash_during_start_fresh_can_be_repeated_until_the_old_files_are_back() {
    let fixture = Fixture::user();
    let original = fixture.tree();
    let crash_before_stage = 6;
    let outcome = fixture.run(&FaultFs::new(Fault::CrashAt(crash_before_stage)));
    assert!(matches!(&outcome, Outcome::Failed(f) if f.can_start_fresh()));
    let j = (0..write_count(Fixture::user))
        .find(|&j| {
            fixture
                .migration(&FaultFs::new(Fault::CrashAt(j)))
                .start_fresh()
                .is_ok()
        })
        .expect("start fresh never finished");
    assert!(j > 2, "start fresh had too few writes to crash at");
    assert!(matches!(
        fixture.run(&RealFs),
        Outcome::NothingToMigrate(Reason::StartedFresh)
    ));
    let mut after = fixture.tree();
    assert!(
        after
            .remove(&format!("local/Navo/{}", super::fixture::MARKER))
            .is_some()
    );
    assert_eq!(after.remove("local/Navo"), Some(None));
    assert!(after == original);
}
