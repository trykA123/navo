use std::collections::HashSet;

use super::*;

fn is_segment(seg: &str) -> bool {
    let mut parts = seg.split('-');
    let head_ok = parts.next().is_some_and(|head| {
        head.starts_with(|c: char| c.is_ascii_lowercase())
            && head
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    });
    head_ok
        && parts.all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

fn matches_id_grammar(name: &str) -> bool {
    match name.split_once('.') {
        Some((a, b)) => is_segment(a) && is_segment(b),
        None => false,
    }
}

#[test]
fn all_holds_nineteen_ids() {
    assert_eq!(CommandId::ALL.len(), 19);
}

#[test]
fn all_holds_each_id_once() {
    let unique: HashSet<_> = CommandId::ALL.iter().collect();
    assert_eq!(unique.len(), CommandId::ALL.len());
}

#[test]
fn names_are_unique() {
    let unique: HashSet<_> = CommandId::ALL.iter().map(|id| id.as_str()).collect();
    assert_eq!(unique.len(), CommandId::ALL.len());
}

#[test]
fn names_match_the_id_grammar() {
    let bad: Vec<_> = CommandId::ALL
        .iter()
        .map(|id| id.as_str())
        .filter(|n| !matches_id_grammar(n))
        .collect();
    assert_eq!(bad, Vec::<&str>::new());
}

#[test]
fn display_is_the_name() {
    let mismatched: Vec<_> = CommandId::ALL
        .iter()
        .filter(|id| id.to_string() != id.as_str())
        .collect();
    assert_eq!(mismatched, Vec::<&CommandId>::new());
}

#[test]
fn from_name_round_trips_every_name() {
    let back: Vec<_> = CommandId::ALL
        .iter()
        .map(|id| CommandId::from_name(id.as_str()))
        .collect();
    let expected: Vec<_> = CommandId::ALL.iter().copied().map(Some).collect();
    assert_eq!(back, expected);
}

#[test]
fn from_name_rejects_unknown_names() {
    let found: Vec<_> = [
        "",
        "pane",
        "pane.fly",
        "Pane.back",
        " pane.back",
        "pane.back ",
    ]
    .iter()
    .filter_map(|n| CommandId::from_name(n))
    .collect();
    assert_eq!(found, Vec::new());
}

#[test]
fn canonical_examples_cover_every_id_in_order() {
    let names: Vec<_> = CANONICAL_EXAMPLES.iter().map(|t| id_part(t)).collect();
    let expected: Vec<_> = CommandId::ALL.iter().map(|id| id.as_str()).collect();
    assert_eq!(names, expected);
}

#[test]
fn command_id_agrees_with_the_parsed_name() {
    let ids: Vec<_> = CANONICAL_EXAMPLES
        .iter()
        .map(|t| parse(t).map(|c| c.id().as_str()))
        .collect();
    let expected: Vec<_> = CANONICAL_EXAMPLES.iter().map(|t| Ok(id_part(t))).collect();
    assert_eq!(ids, expected);
}
