use std::collections::HashSet;

use rusty_store::StoreDirs;
use serde_json::json;

use super::super::{Outcome, RealFs, Reason};
use super::fault_fs::{Fault, FaultFs};
use super::faults::finishes_after_retry;
use super::fixture::{Fixture, Tree, layouts, write_count};

fn record(fixture: &Fixture, version: u32, phase: &str) -> Vec<u8> {
    let dirs = |dirs: &StoreDirs| json!({ "config": dirs.config, "data": dirs.data });
    let record = json!({
        "version": version,
        "phase": phase,
        "old": dirs(&fixture.old),
        "new": dirs(&fixture.new),
    });
    serde_json::to_vec(&record).unwrap()
}

fn corrupt_journals(fixture: &Fixture) -> [Vec<u8>; 3] {
    [
        b"{\"version\": 1, \"pha".to_vec(),
        record(fixture, 1, "exploded"),
        record(fixture, 2, "detached"),
    ]
}

fn take_aside(fixture: &Fixture, bytes: &[u8], label: &str) {
    let aside = format!("{}.corrupt", fixture.journal_rel());
    assert_eq!(fixture.read(&aside).as_deref(), Some(bytes), "{label}");
    std::fs::remove_file(fixture.path(&aside)).unwrap();
}

#[test]
fn a_corrupt_journal_is_kept_aside_and_the_migration_finishes_from_any_state() {
    for make in layouts() {
        let mut seen = HashSet::new();
        for k in 0..write_count(make) {
            for n in 0..corrupt_journals(&make()).len() {
                finishes_unless_seen(make, k, n, &mut seen);
            }
        }
        assert!(seen.len() > 30, "too few distinct states: {}", seen.len());
    }
}

fn finishes_unless_seen(make: fn() -> Fixture, k: usize, n: usize, seen: &mut HashSet<Tree>) {
    let fixture = make();
    let original = fixture.tree();
    let _ = fixture.run(&FaultFs::new(Fault::CrashAt(k)));
    let bytes = &corrupt_journals(&fixture)[n];
    fixture.write(fixture.journal_rel(), bytes);
    if !seen.insert(fixture.state()) {
        return;
    }
    let label = format!("crash at {k}, journal {n}, in place {}", fixture.in_place());
    let outcome = fixture.run(&RealFs);
    assert!(
        matches!(
            outcome,
            Outcome::Done | Outcome::Resumed | Outcome::NothingToMigrate(Reason::AlreadyMigrated)
        ),
        "{label}: {outcome:?}"
    );
    take_aside(&fixture, bytes, &label);
    fixture.assert_one_navo_profile(&original);
    assert!(matches!(fixture.run(&RealFs), Outcome::NothingToMigrate(_)));
}

#[test]
fn start_fresh_with_a_corrupt_journal_restores_or_is_refused_from_any_state() {
    for make in layouts() {
        let mut restored = 0;
        for k in 0..write_count(make) {
            let fixture = make();
            let original = fixture.tree();
            let _ = fixture.run(&FaultFs::new(Fault::CrashAt(k)));
            let bytes = corrupt_journals(&fixture)[0].clone();
            fixture.write(fixture.journal_rel(), &bytes);
            let label = format!("crash at {k}, in place {}", fixture.in_place());
            let result = fixture.migration(&RealFs).start_fresh();
            take_aside(&fixture, &bytes, &label);
            if result.is_ok() {
                fixture.assert_started_fresh(&original, &[]);
                restored += 1;
            } else {
                finishes_after_retry(&fixture, &original, &label);
            }
        }
        assert!(restored > 5, "too few states allow start fresh");
    }
}
