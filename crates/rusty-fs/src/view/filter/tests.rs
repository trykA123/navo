use rusty_core::ProviderPath;

use super::{ViewFilter, filter_order, materialize};
use crate::view::Needle;
use crate::{Attributes, CancelToken, EntryKind, FsEntry};

fn entry(name: &str, hidden: bool, system: bool) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind: EntryKind::File,
        size: 0,
        modified_ms: None,
        attributes: Attributes {
            hidden,
            system,
            ..Attributes::default()
        },
    }
}

fn rows() -> Vec<FsEntry> {
    vec![
        entry("alpha.txt", false, false),
        entry("beta.rs", true, false),
        entry("gamma.sys", true, true),
        entry(".navo-mutations-1", false, false),
        entry("alpine.md", false, false),
    ]
}

fn run(rows: &[FsEntry], needle: Option<&Needle>, hidden: bool, system: bool) -> Vec<String> {
    let order: Vec<u32> = (0..u32::try_from(rows.len()).unwrap()).collect();
    let filter = ViewFilter {
        needle,
        show_hidden: hidden,
        show_system: system,
    };
    let kept = filter_order(rows, &order, &filter, &CancelToken::new()).unwrap();
    materialize(rows, &kept)
        .iter()
        .map(|e| e.name.to_text())
        .collect()
}

#[test]
fn hidden_and_system_are_dropped_by_default() {
    assert_eq!(run(&rows(), None, false, false), ["alpha.txt", "alpine.md"]);
}

#[test]
fn hidden_shown_without_system() {
    assert_eq!(
        run(&rows(), None, true, false),
        ["alpha.txt", "beta.rs", "alpine.md"]
    );
}

#[test]
fn everything_shown_still_drops_reserved_names() {
    assert_eq!(
        run(&rows(), None, true, true),
        ["alpha.txt", "beta.rs", "gamma.sys", "alpine.md"]
    );
}

#[test]
fn a_needle_filters_names() {
    let needle = Needle::parse("alp");
    assert_eq!(
        run(&rows(), Some(&needle), true, true),
        ["alpha.txt", "alpine.md"]
    );
}

#[test]
fn caret_anchors_and_star_matches_gaps() {
    let anchored = Needle::parse("^be");
    assert_eq!(run(&rows(), Some(&anchored), true, true), ["beta.rs"]);
    let starred = Needle::parse("a*.md");
    assert_eq!(run(&rows(), Some(&starred), true, true), ["alpine.md"]);
}

#[test]
fn the_order_is_respected() {
    let rows = rows();
    let filter = ViewFilter {
        needle: None,
        show_hidden: true,
        show_system: true,
    };
    let kept = filter_order(&rows, &[4, 0], &filter, &CancelToken::new()).unwrap();
    assert_eq!(kept, [4, 0]);
}

#[test]
fn a_cancelled_token_returns_none() {
    let cancel = CancelToken::new();
    cancel.cancel();
    let filter = ViewFilter {
        needle: None,
        show_hidden: true,
        show_system: true,
    };
    assert!(filter_order(&rows(), &[0, 1], &filter, &cancel).is_none());
}
