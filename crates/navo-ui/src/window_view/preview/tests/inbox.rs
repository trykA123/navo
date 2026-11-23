//! Tests for the preview inbox: scan snapshots share one latest-wins slot, so a slow
//! window holds at most one snapshot, while reads, counts and totals keep their channel.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use navo_fs::{FolderProfile, LiveSink, LiveSnapshot, ProfileState, Ranking, Tally};

use super::super::inbox::{Arrival, Inbox, ReadDone};
use super::super::scan::Snapshots;

/// Builds an inbox whose waker counts its calls; returns both.
fn counted_inbox() -> (Inbox, Arc<AtomicUsize>) {
    let wakes = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&wakes);
    let inbox = Inbox::new(Arc::new(move || {
        seen.fetch_add(1, Ordering::SeqCst);
    }));
    (inbox, wakes)
}

/// A scan sink for `run` that writes into this inbox, as `FolderPreview::scan` builds it.
fn sink(inbox: &Inbox, run: u64) -> Snapshots {
    Snapshots {
        latest: inbox.latest(),
        wake: Arc::clone(&inbox.wake),
        run,
        every: Duration::from_millis(1),
    }
}

/// A running snapshot whose `counts` tells snapshots apart.
fn live(counts: u64) -> LiveSnapshot {
    LiveSnapshot {
        profile: FolderProfile {
            totals: Default::default(),
            newest_ms: None,
            kinds: Vec::new(),
            other: Tally::default(),
            largest: Vec::new(),
            ranking: Ranking::default(),
            counted_ms: None,
            state: ProfileState::Running,
        },
        is_live: true,
        is_recounting: false,
        counts,
        changes: 0,
    }
}

/// Lists the arrivals as `(kind, run or read id, counts)` so asserts stay short.
fn summary(arrivals: Vec<Arrival>) -> Vec<(&'static str, u64, u64)> {
    arrivals
        .into_iter()
        .map(|arrival| match arrival {
            Arrival::Snapshot { run, snapshot } => ("snapshot", run, snapshot.counts),
            Arrival::Read(done) => ("read", done.read, 0),
            Arrival::Counted { bytes, .. } => ("counted", bytes, 0),
            Arrival::Totals { load, .. } => ("totals", load, 0),
        })
        .collect()
}

#[test]
fn ten_thousand_snapshots_without_a_take_leave_only_the_newest() {
    let (inbox, wakes) = counted_inbox();
    let mut scan = sink(&inbox, 3);
    for counts in 0..10_000 {
        scan.snapshot(&live(counts));
        if counts == 5_000 {
            let read = Arrival::Read(ReadDone {
                load: 1,
                read: 42,
                node: 0,
                listed: Err("gone".into()),
            });
            inbox.send.send(read).expect("the inbox is alive");
        }
    }
    assert_eq!(wakes.load(Ordering::SeqCst), 10_000);
    assert_eq!(
        summary(inbox.take()),
        [("snapshot", 3, 9_999), ("read", 42, 0)]
    );
    assert!(
        inbox.take().is_empty(),
        "take empties the slot and the channel"
    );
}

#[test]
fn a_late_snapshot_from_an_older_run_keeps_the_newer_run() {
    let (inbox, _) = counted_inbox();
    sink(&inbox, 8).snapshot(&live(1));
    sink(&inbox, 7).snapshot(&live(2));
    assert_eq!(summary(inbox.take()), [("snapshot", 8, 1)]);
    sink(&inbox, 7).snapshot(&live(3));
    assert_eq!(summary(inbox.take()), [("snapshot", 7, 3)]);
}

#[test]
fn a_snapshot_after_the_inbox_is_gone_does_not_wake() {
    let (inbox, wakes) = counted_inbox();
    let mut scan = sink(&inbox, 1);
    drop(inbox);
    scan.snapshot(&live(1));
    assert_eq!(wakes.load(Ordering::SeqCst), 0);
}
