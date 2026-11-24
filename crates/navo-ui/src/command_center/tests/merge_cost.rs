//! Cost guard for `Results::merge`, which runs on the UI thread each time Ctrl+K results arrive.

use std::time::Instant;

use navo_core::Location;

use crate::command_center::places::take_fold_keys;
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

/// Merges `here` folder-walk hits and `index` hits with the same paths; returns the row count.
fn merge_both(here: usize, index: usize) -> usize {
    let mut hits = windows_hits("here", here);
    hits.extend(windows_hits("index", index));
    Results::merge(hits, Scope::Files).len()
}

// The pairwise merge folded two keys per pair of places: about 500k keys for 1000 hits.
#[test]
fn merging_a_full_page_of_windows_file_hits_folds_each_path_once() {
    take_fold_keys();
    assert_eq!(
        merge_both(500, 500),
        501,
        "one header and one row per place"
    );
    let keys = take_fold_keys();
    assert!(
        keys <= 1_000,
        "merging 1000 hits built {keys} folded path keys; at most one per hit keeps it linear"
    );
}

#[test]
#[ignore = "timing measurement, run by hand in a release build"]
#[expect(
    clippy::print_stdout,
    reason = "a by-hand timing run prints its numbers"
)]
fn merge_timing_for_500_and_500_plus_500_hits() {
    for (here, index) in [(500, 0), (500, 500)] {
        let best = (0..5)
            .map(|_| {
                let start = Instant::now();
                merge_both(here, index);
                start.elapsed()
            })
            .min()
            .expect("five runs");
        println!("merge {here} here + {index} index hits: {best:?}");
    }
}
