//! The network limit: stop, keep the rows, continue, cancel.

use std::collections::BTreeSet;
use std::time::Duration;

use super::fake::{FakeSource, SlowSpec};
use super::*;

const QUIET: Duration = Duration::from_millis(300);

fn tree(network: bool, slow: Option<Duration>) -> (FakeSource, FakeSource) {
    let build = || {
        let mut side = FakeSource::new(true);
        if network {
            side.set_network();
        }
        side
    };
    let (mut left, mut right) = (build(), build());
    for folder in 0..4 {
        for side in [&mut left, &mut right] {
            side.add_folder(&format!("d{folder}"));
            for file in 0..10 {
                side.add_file(&format!("d{folder}/f{file}.txt"), vec![1, 2, 3]);
            }
        }
    }
    for name in ["s0", "s1", "s2"] {
        let Some(delay) = slow else { break };
        let started = Arc::default();
        left.add_slow_folder(
            name,
            SlowSpec {
                count: 1,
                every: 1,
                delay,
                started,
            },
        );
    }
    (left, right)
}

fn capped(entries: u64, duration: Duration) -> SessionOptions {
    SessionOptions {
        network_cap: Some(NetworkCap { entries, duration }),
        ..options(CheckMode::SizeTime)
    }
}

fn run(sides: (FakeSource, FakeSource), options: SessionOptions) -> Session {
    Session::start(Arc::new(sides.0), Arc::new(sides.1), options)
}

/// Batches up to and including the first one that is final or reports a stop.
fn until_stop(session: &Session) -> Vec<RowBatch> {
    let mut batches = Vec::new();
    loop {
        let batch = session
            .batches()
            .recv_timeout(Duration::from_secs(20))
            .expect("session should emit a batch");
        let done = batch.is_final || batch.limit.is_some();
        batches.push(batch);
        if done {
            return batches;
        }
    }
}

fn names(batches: &[RowBatch]) -> Vec<String> {
    batches
        .iter()
        .flat_map(|batch| batch.rows.iter())
        .map(row_name)
        .collect()
}

fn uncapped_names() -> Vec<String> {
    let session = run(tree(true, None), options(CheckMode::SizeTime));
    let mut names = names(&collect(&session));
    names.sort();
    names
}

#[test]
fn entry_cap_stops_and_keeps_the_rows_found() {
    let session = run(tree(true, None), capped(30, Duration::from_secs(600)));
    let batches = until_stop(&session);
    let last = batches.last().unwrap();

    assert!(!last.is_final);
    let stop = last.limit.expect("the walk should report a stop");
    assert_eq!(stop.bound, StopBound::Entries);
    // Root lists 8 entries, d0 and d1 list 20 each, then 48 >= 30 stops the walk.
    assert_eq!(stop.entries, 48);
    assert_eq!(last.totals.rows, 24);
    assert_eq!(names(&batches).len(), 24);
    assert!(session.batches().recv_timeout(QUIET).is_err());
}

#[test]
fn continue_walk_finishes_with_the_uncapped_rows() {
    let session = run(tree(true, None), capped(30, Duration::from_secs(600)));
    let mut seen = names(&until_stop(&session));
    assert!(session.continue_walk());
    assert!(!session.continue_walk());
    // Fresh bounds: d2 and d3 list 40 entries, but nothing is left after d3, so no second stop.
    let batches = until_stop(&session);
    seen.extend(names(&batches));
    let last = batches.last().unwrap();
    assert!(last.is_final && last.limit.is_none());
    assert_eq!(last.totals.rows, 44);
    seen.sort();
    assert_eq!(seen, uncapped_names());
    assert_eq!(seen.iter().collect::<BTreeSet<_>>().len(), seen.len());
}

#[test]
fn time_cap_stops_between_folders_and_continue_restarts_the_clock() {
    let slow = Some(Duration::from_millis(60));
    let session = run(
        tree(true, slow),
        capped(1_000_000, Duration::from_millis(20)),
    );
    let mut seen = Vec::new();
    let mut bounds = Vec::new();
    loop {
        let batches = until_stop(&session);
        seen.extend(names(&batches));
        let last = batches.last().unwrap();
        if last.is_final {
            break;
        }
        bounds.push(last.limit.unwrap().bound);
        assert!(session.continue_walk());
    }
    assert!(!bounds.is_empty());
    assert!(bounds.iter().all(|bound| *bound == StopBound::Time));
    seen.sort();
    let flat = run(
        tree(true, Some(Duration::ZERO)),
        options(CheckMode::SizeTime),
    );
    let mut expected = names(&collect(&flat));
    expected.sort();
    assert_eq!(seen, expected);
}

#[test]
fn cancel_while_stopped_ends_the_walk() {
    let session = run(tree(true, None), capped(30, Duration::from_secs(600)));
    let stopped = until_stop(&session);
    assert!(stopped.last().unwrap().limit.is_some());

    session.cancel();
    let last = until_stop(&session).pop().unwrap();
    assert!(last.is_final);
    assert!(last.limit.is_none());
    assert!(!session.continue_walk());
}

#[test]
fn refresh_while_stopped_starts_a_new_generation() {
    let mut session = run(tree(true, None), capped(30, Duration::from_secs(600)));
    assert!(until_stop(&session).last().unwrap().limit.is_some());
    session.refresh();
    let last = until_stop(&session).pop().unwrap();
    assert_eq!(last.generation, Generation::new(2));
    assert_eq!(last.limit.map(|stop| stop.bound), Some(StopBound::Entries));
}

#[test]
fn a_side_that_is_not_on_a_network_ignores_the_cap() {
    let session = run(tree(false, None), capped(1, Duration::ZERO));
    let batches = collect(&session);
    assert!(batches.iter().all(|batch| batch.limit.is_none()));
    assert_eq!(batches.last().unwrap().totals.rows, 44);
}

#[test]
fn no_cap_means_no_stop_on_a_network() {
    let session = run(tree(true, None), options(CheckMode::SizeTime));
    let batches = collect(&session);
    assert!(batches.iter().all(|batch| batch.limit.is_none()));
    assert_eq!(batches.last().unwrap().totals.rows, 44);
}
