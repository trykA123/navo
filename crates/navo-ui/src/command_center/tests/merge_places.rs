//! Equivalence of the hashed `one_per_place` with the old pairwise merge it replaced.

use navo_core::Location;
use proptest::prelude::*;

use crate::command_center::results::one_per_place;
use crate::command_center::{Group, Hit, Target};

const SOURCES: [&str; 6] = ["tabs", "recent", "paths", "here", "index", "commands"];

// Mixed-case Windows drive and UNC paths, a lowercase drive, a dotless-i "drive" that does not
// fold, POSIX paths and a remote provider; each family has entries that differ only in case.
const PATHS: [&str; 14] = [
    r"C:\Dir\a.txt",
    r"c:\dir\A.TXT",
    r"C:\",
    r"c:\",
    r"D:\x",
    r"\\Srv\Share\f",
    r"\\srv\share\F",
    r"\\SRV\SHARE",
    "I:\\X",
    "ı:\\x",
    "/home/a/B",
    "/home/a/b",
    "sftp://host/A",
    "sftp://host/a",
];

/// The pairwise merge as it was before the hashed lookup: the reference result.
#[expect(
    clippy::too_many_lines,
    reason = "a verbatim copy of the replaced merge"
)]
fn pairwise(mut hits: Vec<Hit>) -> Vec<Hit> {
    let place_of = |hit: &Hit| -> Option<Location> {
        match (&hit.target, hit.source) {
            (Target::GoTo(location), "tabs" | "paths")
            | (Target::Open { location, .. }, "recent" | "paths" | "here" | "index") => {
                Some(location.clone())
            }
            _ => None,
        }
    };
    let of = |hits: &[Hit], source: &str| -> Vec<usize> {
        (0..hits.len())
            .filter(|&at| hits[at].source == source && place_of(&hits[at]).is_some())
            .collect()
    };
    let mut dropped = vec![false; hits.len()];
    let mut absorb = |hits: &mut [Hit], kept: &[usize], at: usize| {
        let winner = kept.iter().copied().find(|&win| {
            matches!((place_of(&hits[win]), place_of(&hits[at])), (Some(a), Some(b)) if a.same_place(&b))
        });
        if let Some(win) = winner {
            hits[win].score = hits[win].score.max(hits[at].score);
            dropped[at] = true;
        }
        winner.is_none()
    };
    let mut kept: Vec<usize> = Vec::new();
    for at in of(&hits, "tabs") {
        if absorb(&mut hits, &kept, at) {
            kept.push(at);
        }
    }
    let recents: Vec<usize> = of(&hits, "recent")
        .into_iter()
        .filter(|&at| absorb(&mut hits, &kept, at))
        .collect();
    kept.extend(recents);
    let paths: Vec<usize> = of(&hits, "paths")
        .into_iter()
        .filter(|&at| absorb(&mut hits, &kept, at))
        .collect();
    kept.extend(paths);
    for source in ["here", "index"] {
        for at in of(&hits, source) {
            if absorb(&mut hits, &kept, at) {
                kept.push(at);
            }
        }
    }
    let mut at = 0;
    hits.retain(|_| {
        at += 1;
        !dropped[at - 1]
    });
    hits
}

/// Builds one hit from `source` for `path`, as a go-to or an open target.
fn hit(source: &'static str, path: &str, score: i64, go_to: bool) -> Hit {
    let location = Location::new(path).expect("a fixture path");
    Hit {
        source,
        group: Group::Files,
        title: path.to_owned().into(),
        detail: format!("{source} {score}").into(),
        score,
        indices: Vec::new(),
        target: if go_to {
            Target::GoTo(location)
        } else {
            Target::Open {
                location,
                folder: false,
            }
        },
        shortcut: None,
    }
}

#[test]
fn index_and_walk_duplicates_keep_the_first_place_with_the_best_score() {
    let hits = vec![
        hit("index", r"c:\dir\A.TXT", 9, false),
        hit("here", r"C:\Dir\a.txt", 3, false),
        hit("tabs", r"\\srv\share\F", 1, true),
        hit("index", r"\\Srv\Share\f", 7, false),
        hit("here", "/home/a/b", 2, false),
        hit("index", "/home/a/B", 4, false),
        hit("index", "sftp://host/a", 5, false),
        hit("here", "sftp://host/A", 6, false),
    ];
    let merged = one_per_place(hits.clone());
    assert_eq!(
        merged,
        pairwise(hits),
        "the hashed merge must match the pairwise one"
    );
    let kept: Vec<(&str, i64)> = merged.iter().map(|h| (h.source, h.score)).collect();
    assert_eq!(
        kept,
        [
            ("here", 9),
            ("tabs", 7),
            ("here", 2),
            ("index", 4),
            ("index", 5),
            ("here", 6)
        ],
        "Windows paths fold case, POSIX and remote paths stay exact"
    );
}

#[test]
fn recents_that_share_a_place_both_stay_and_the_first_absorbs_later_hits() {
    let hits = vec![
        hit("recent", r"C:\Dir\a.txt", 1, false),
        hit("recent", r"c:\dir\A.TXT", 2, false),
        hit("index", r"C:\DIR\A.TXT", 8, false),
    ];
    let merged = one_per_place(hits.clone());
    assert_eq!(
        merged,
        pairwise(hits),
        "the hashed merge must match the pairwise one"
    );
    let scores: Vec<i64> = merged.iter().map(|h| h.score).collect();
    assert_eq!(
        scores,
        [8, 2],
        "only the first kept recent takes the index score"
    );
}

/// One generated hit: a source, a path from `PATHS`, a score and the target kind.
fn any_hit() -> impl Strategy<Value = Hit> {
    (0..SOURCES.len(), 0..PATHS.len(), 0i64..6, any::<bool>())
        .prop_map(|(source, path, score, go_to)| hit(SOURCES[source], PATHS[path], score, go_to))
}

proptest! {
    #[test]
    fn hashed_merge_matches_the_pairwise_merge(hits in prop::collection::vec(any_hit(), 0..24)) {
        prop_assert_eq!(one_per_place(hits.clone()), pairwise(hits));
    }
}
