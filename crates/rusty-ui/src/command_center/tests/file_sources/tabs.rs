use rusty_core::{AppState, Location};

use crate::command_center::{
    Cancel, Group, Hit, HitSink, Query, Request, ResultSource, TabsSnapshot, TabsSource, Target,
};

fn place(text: &str) -> Location {
    Location::new(text).unwrap()
}

fn run(source: &TabsSource, input: &str) -> Vec<Hit> {
    let request = Request {
        query: Query::parse(input),
        generation: 1,
        here: None,
        limit: 100,
    };
    let sink = HitSink::new(1);
    source.search(&request, &sink, &Cancel::new());
    sink.drain(1)
}

fn source() -> TabsSource {
    TabsSource::new(TabsSnapshot::from_locations([
        place("/work/ferrite"),
        place("/work/invoices"),
        place("/srv/ferrite-backup"),
    ]))
}

#[test]
fn matching_tabs_become_go_to_folder_hits() {
    let hits = run(&source(), "ferrite");
    assert_eq!(hits.len(), 2);
    for hit in &hits {
        assert_eq!(hit.source, "tabs");
        assert_eq!(hit.group, Group::Folders);
        assert!(matches!(&hit.target, Target::GoTo(_)));
    }
    let first = hits.iter().find(|hit| hit.title == "ferrite").unwrap();
    assert_eq!(first.detail, "/work");
    assert_eq!(first.target, Target::GoTo(place("/work/ferrite")));
}

#[test]
fn a_path_term_matches_the_whole_location() {
    let hits = run(&source(), "srv/");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].title, "ferrite-backup");
}

#[test]
fn ext_terms_and_blank_queries_match_no_tab() {
    assert!(run(&source(), "ext:rs").is_empty());
    assert!(run(&source(), "").is_empty());
}

#[test]
fn the_snapshot_lists_the_tab_of_every_pane() {
    let state = AppState::new(place("/home"));
    let source = TabsSource::new(TabsSnapshot::of(&state));
    assert_eq!(run(&source, "home").len(), 1);
}
