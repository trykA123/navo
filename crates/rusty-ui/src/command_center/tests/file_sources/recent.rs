use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::command_center::{
    Cancel, Group, Hit, HitSink, Query, RecentSource, Request, ResultSource, Scopes, Target,
};
use crate::window_view::Frecent;

fn row(location: &str, is_folder: bool, weight: f64) -> Frecent {
    Frecent {
        location: location.to_owned(),
        is_folder,
        weight,
    }
}

fn rows() -> Vec<Frecent> {
    vec![
        row("/work/ferrite", true, 40.0),
        row("/work/notes/budget.xlsx", false, 20.0),
        row("/work/budget", true, 2.0),
        row("/work/src/main.rs", false, 1.0),
        row("", true, 90.0),
    ]
}

fn source() -> RecentSource {
    RecentSource::new(Arc::new(|_| rows()))
}

fn run(source: &RecentSource, input: &str, cancel: &Cancel) -> Vec<Hit> {
    let request = Request {
        query: Query::parse(input),
        generation: 1,
        here: None,
        limit: 100,
    };
    let sink = HitSink::new(1);
    source.search(&request, &sink, cancel);
    sink.drain(1)
}

fn titles(hits: &[Hit]) -> Vec<String> {
    hits.iter().map(|hit| hit.title.to_string()).collect()
}

#[test]
fn folders_and_files_come_back_in_their_groups_with_open_targets() {
    let hits = run(&source(), "budget", &Cancel::new());
    let file = hits.iter().find(|hit| hit.group == Group::Files).unwrap();
    assert_eq!(file.title, "budget.xlsx");
    assert_eq!(file.detail, "/work/notes");
    assert!(matches!(&file.target, Target::Open { folder: false, .. }));
    let folder = hits.iter().find(|hit| hit.group == Group::Folders).unwrap();
    assert!(matches!(&folder.target, Target::Open { folder: true, .. }));
    assert!(hits.iter().all(|hit| hit.source == "recent"));
}

#[test]
fn a_heavier_weight_ranks_higher_for_the_same_match() {
    let hits = run(&source(), "budget", &Cancel::new());
    let score = |title: &str, group| {
        hits.iter()
            .find(|hit| hit.title == title && hit.group == group)
            .map(|hit| hit.score)
            .unwrap()
    };
    assert!(score("budget.xlsx", Group::Files) > score("budget", Group::Folders));
}

#[test]
fn every_term_must_match_and_ext_and_path_terms_filter() {
    let none = run(&source(), "budget ferrite", &Cancel::new());
    assert!(none.is_empty());
    let hits = run(&source(), "ext:rs", &Cancel::new());
    assert_eq!(titles(&hits), ["main.rs"]);
    assert!(run(&source(), "ext:txt", &Cancel::new()).is_empty());
    let hits = run(&source(), "main ext:rs", &Cancel::new());
    assert_eq!(titles(&hits), ["main.rs"]);
    let hits = run(&source(), "notes/", &Cancel::new());
    assert_eq!(titles(&hits), ["budget.xlsx"]);
}

#[test]
fn blank_and_path_mode_queries_give_no_hits() {
    assert!(run(&source(), "", &Cancel::new()).is_empty());
    assert!(run(&source(), "C:\\Users\\", &Cancel::new()).is_empty());
}

#[test]
fn the_name_match_is_highlighted() {
    let hits = run(&source(), "ferr", &Cancel::new());
    assert_eq!(hits[0].title, "ferrite");
    assert_eq!(hits[0].indices, [0, 1, 2, 3]);
}

#[test]
fn a_cancelled_search_pushes_nothing() {
    let cancel = Cancel::new();
    cancel.cancel();
    assert!(run(&source(), "budget", &cancel).is_empty());
}

#[test]
fn history_loads_once_with_the_2000_row_limit_and_skips_bad_locations() {
    let calls = Arc::new(AtomicUsize::new(0));
    let asked = Arc::new(AtomicUsize::new(0));
    let (count, limit) = (calls.clone(), asked.clone());
    let source = RecentSource::new(Arc::new(move |n| {
        count.fetch_add(1, Ordering::SeqCst);
        limit.store(n, Ordering::SeqCst);
        rows()
    }));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    run(&source, "budget", &Cancel::new());
    run(&source, "main", &Cancel::new());
    assert_eq!(source.top_folders(8).len(), 2);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(asked.load(Ordering::SeqCst), 2_000);
}

#[test]
fn top_folders_keeps_the_stored_order_and_the_count() {
    let top = source().top_folders(1);
    assert_eq!(titles(&top), ["ferrite"]);
    assert_eq!(titles(&source().top_folders(8)), ["ferrite", "budget"]);
}

#[test]
fn recent_searches_the_files_scopes() {
    assert_eq!(source().scopes(), Scopes::FILES);
}
