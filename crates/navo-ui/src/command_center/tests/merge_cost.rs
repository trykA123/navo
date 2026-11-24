//! Cost guard for `Results::merge`, which runs on the UI thread each time Ctrl+K results arrive.

use std::time::{Duration, Instant};

use navo_core::Location;

use crate::command_center::{Group, Hit, Results, Scope, Target};

/// Builds `count` file hits from `source` with local Windows paths, as the index and the
/// folder walk report them.
fn windows_hits(source: &'static str, count: usize) -> Vec<Hit> {
    (0..count)
        .map(|n| {
            let path = format!(r"C:\Users\admin\Projects\workspace\src\module{n}\report_{n}.txt");
            Hit {
                source,
                group: Group::Files,
                title: format!("report_{n}.txt").into(),
                detail: path.clone().into(),
                score: 1_000 - n as i64,
                indices: Vec::new(),
                target: Target::Open {
                    location: Location::new(&path).expect("a Windows path"),
                    folder: false,
                },
                shortcut: None,
            }
        })
        .collect()
}

// A linear merge of 1000 hits takes a few ms even in debug; the pairwise one took 1.3 s.
#[test]
fn merging_a_full_page_of_windows_file_hits_is_not_quadratic() {
    let best = (0..3)
        .map(|_| {
            let mut hits = windows_hits("here", 500);
            hits.extend(windows_hits("index", 500));
            let start = Instant::now();
            let rows = Results::merge(hits, Scope::Files);
            assert_eq!(rows.len(), 501);
            start.elapsed()
        })
        .min()
        .expect("three runs");
    assert!(
        best < Duration::from_millis(250),
        "merging 1000 hits took {best:?}: one_per_place compares every pair of places"
    );
}
