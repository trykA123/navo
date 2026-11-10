//! Tests for searching the Stream: matching, progress, cancellation and clearing.

use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use navo_ui::compare_diff::{StreamControl, StreamUpdate};

use super::stream::support::{PairData, channel, items, load_fn, not_viewed};
use crate::compare::diff::stream::StreamSession;

const WAIT: Duration = Duration::from_secs(20);
const BUDGET: u64 = 64 * 1024 * 1024;

/// One in-memory pair.
fn pair(left: &[u8], right: &[u8]) -> PairData {
    PairData {
        left: left.to_vec(),
        right: right.to_vec(),
        blocked: false,
    }
}

/// Start a Stream over `data` with an optional read delay.
fn start(data: HashMap<u64, PairData>, delay: Duration) -> (StreamSession, Receiver<StreamUpdate>) {
    let (sink, receiver) = channel();
    let load = load_fn(data, delay, None, None);
    let session = StreamSession::start(sink, BUDGET, load, not_viewed());
    (session, receiver)
}

/// The next `Hits` update for `query`, skipping other updates.
fn next_hits(
    receiver: &Receiver<StreamUpdate>,
    query: u64,
) -> (Vec<(u64, Vec<u32>)>, u32, u32, bool) {
    loop {
        let update = receiver.recv_timeout(WAIT).expect("a Hits update");
        if let StreamUpdate::Hits {
            query: found,
            hits,
            searched,
            total,
            done,
            ..
        } = update
            && found == query
        {
            return (hits, searched, total, done);
        }
    }
}

/// Wait for the final `Hits` of `query` and merge the parts into one map.
fn wait_done(receiver: &Receiver<StreamUpdate>, query: u64) -> (HashMap<u64, Vec<u32>>, u32, u32) {
    let mut all = HashMap::new();
    loop {
        let (hits, searched, total, done) = next_hits(receiver, query);
        all.extend(hits);
        if done {
            return (all, searched, total);
        }
    }
}

#[test]
fn search_is_case_insensitive_over_changed_lines_and_paths() {
    let mut data = HashMap::new();
    data.insert(
        1,
        pair(b"alpha\nneedle\nomega\n", b"alpha\nNEEDLE\nomega\n"),
    );
    data.insert(2, pair(b"same\n", b"same\n"));
    data.insert(3, pair(b"other\n", b"changed\n"));
    let (session, receiver) = start(data.clone(), Duration::ZERO);
    let mut list = items(&data, true);
    for (request, _) in &mut list {
        if request.id == 2 {
            request.path = "Needle-path.txt".to_owned();
        }
    }
    session.set_items(1, list);
    session.search(5, "nEeDlE".to_owned());
    let (hits, searched, total) = wait_done(&receiver, 5);
    assert_eq!(total, 3);
    assert_eq!(searched, 3);
    assert_eq!(hits.get(&1), Some(&vec![0]));
    assert_eq!(hits.get(&2), Some(&Vec::new()));
    assert!(!hits.contains_key(&3));
}

#[test]
fn context_lines_never_match() {
    let mut data = HashMap::new();
    data.insert(
        1,
        pair(
            b"needle context\nold value\nsame\n",
            b"needle context\nnew value\nsame\n",
        ),
    );
    let (session, receiver) = start(data.clone(), Duration::ZERO);
    session.set_items(1, items(&data, true));
    session.search(6, "needle".to_owned());
    let (hits, searched, total) = wait_done(&receiver, 6);
    assert_eq!(searched, 1);
    assert_eq!(total, 1);
    assert!(hits.is_empty(), "a context line matched: {hits:?}");
}

#[test]
fn search_sends_progress_before_the_done_update() {
    let data: HashMap<u64, PairData> = (0..12)
        .map(|id| (id, pair(b"alpha\n", b"ALPHA changed\n")))
        .collect();
    let (session, receiver) = start(data.clone(), Duration::from_millis(30));
    session.set_items(1, items(&data, true));
    session.search(9, "alpha".to_owned());
    let mut partial = false;
    loop {
        let (_, searched, total, done) = next_hits(&receiver, 9);
        assert_eq!(total, 12);
        if done {
            assert_eq!(searched, 12);
            break;
        }
        assert!(searched <= 12, "a partial update searched too many files");
        partial = true;
    }
    assert!(partial, "no partial Hits update arrived");
}

#[test]
fn a_newer_query_cancels_the_older() {
    let data: HashMap<u64, PairData> = (0..20)
        .map(|id| (id, pair(b"alpha\n", b"ALPHA\n")))
        .collect();
    let (session, receiver) = start(data.clone(), Duration::from_millis(40));
    session.set_items(1, items(&data, true));
    session.search(1, "alpha".to_owned());
    let (_, searched, _, done) = next_hits(&receiver, 1);
    assert!(!done && searched < 20, "the first query finished too soon");
    session.search(2, "beta".to_owned());
    let mut old_done = false;
    loop {
        match receiver.recv_timeout(WAIT).expect("a Hits update") {
            StreamUpdate::Hits {
                query: 1,
                done: true,
                ..
            } => old_done = true,
            StreamUpdate::Hits {
                query: 2,
                done: true,
                ..
            } => break,
            _ => {}
        }
    }
    assert!(!old_done, "the older query finished after cancellation");
}

#[test]
fn empty_text_stops_the_search() {
    let data: HashMap<u64, PairData> = (0..20)
        .map(|id| (id, pair(b"alpha\n", b"ALPHA\n")))
        .collect();
    let (session, receiver) = start(data.clone(), Duration::from_millis(40));
    session.set_items(1, items(&data, true));
    session.search(1, "alpha".to_owned());
    let (_, searched, _, done) = next_hits(&receiver, 1);
    assert!(!done && searched < 20, "the first query finished too soon");
    session.search(2, String::new());
    let until = Instant::now() + Duration::from_millis(500);
    while Instant::now() < until {
        let Ok(update) = receiver.recv_timeout(Duration::from_millis(50)) else {
            continue;
        };
        if let StreamUpdate::Hits {
            query, done: true, ..
        } = update
        {
            panic!("query {query} sent a final update after the search was cleared");
        }
    }
    session.search(3, "alpha".to_owned());
    let (_, searched, total) = wait_done(&receiver, 3);
    assert_eq!(searched, 20);
    assert_eq!(total, 20);
}
