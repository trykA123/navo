use std::sync::Arc;
use std::thread;

use rusty_core::{Command, Location};

use crate::commands::ArgKind;

use crate::command_center::{
    Cancel, Group, Hit, HitSink, Query, Request, ResultSource, Scope, Scopes, Target,
};

fn hit(title: &str) -> Hit {
    Hit {
        source: "test",
        group: Group::Files,
        title: title.to_owned().into(),
        detail: "".into(),
        score: 10,
        indices: vec![0],
        target: Target::Action("window.close"),
        shortcut: None,
    }
}

fn titles(hits: &[Hit]) -> Vec<String> {
    hits.iter().map(|hit| hit.title.to_string()).collect()
}

fn request(generation: u64) -> Request {
    Request {
        query: Query::parse("a"),
        generation,
        here: None,
        limit: 2,
    }
}

struct Fixed;

impl ResultSource for Fixed {
    fn id(&self) -> &'static str {
        "fixed"
    }

    fn group(&self) -> Group {
        Group::Module("Fixed")
    }

    fn scopes(&self) -> Scopes {
        Scopes::FILES
    }

    fn search(&self, request: &Request, sink: &HitSink, cancel: &Cancel) {
        for index in 0..request.limit {
            if cancel.is_cancelled() {
                return;
            }
            sink.push(request.generation, hit(&format!("hit {index}")));
        }
    }
}

#[test]
fn sink_keeps_hits_of_the_current_generation() {
    let sink = HitSink::new(3);
    assert!(sink.push(3, hit("a")));
    assert!(sink.push(3, hit("b")));
    assert_eq!(titles(&sink.drain(3)), ["a", "b"]);
    assert!(sink.drain(3).is_empty());
}

#[test]
fn sink_drops_hits_of_a_stale_generation() {
    let sink = HitSink::new(5);
    assert!(!sink.push(4, hit("late")));
    assert!(!sink.push(6, hit("early")));
    assert!(sink.drain(5).is_empty());
}

#[test]
fn begin_starts_a_new_generation_and_discards_old_hits() {
    let sink = HitSink::new(1);
    sink.push(1, hit("old"));
    sink.begin(2);
    assert_eq!(sink.generation(), 2);
    assert!(!sink.push(1, hit("late")));
    assert!(sink.push(2, hit("new")));
    assert!(sink.drain(1).is_empty());
    assert_eq!(titles(&sink.drain(2)), ["new"]);
}

#[test]
fn begin_never_goes_back_to_an_older_generation() {
    let sink = HitSink::new(7);
    sink.push(7, hit("kept"));
    sink.begin(7);
    sink.begin(3);
    assert_eq!(sink.generation(), 7);
    assert_eq!(titles(&sink.drain(7)), ["kept"]);
}

#[test]
fn clones_share_one_collector_across_threads() {
    let sink = HitSink::new(1);
    let workers: Vec<_> = (0..4)
        .map(|worker| {
            let sink = sink.clone();
            thread::spawn(move || {
                for index in 0..25 {
                    sink.push(1, hit(&format!("{worker}-{index}")));
                }
            })
        })
        .collect();
    for worker in workers {
        assert!(worker.join().is_ok());
    }
    assert_eq!(sink.drain(1).len(), 100);
}

#[test]
fn cancel_is_shared_between_clones() {
    let cancel = Cancel::new();
    let seen = cancel.clone();
    assert!(!seen.is_cancelled());
    cancel.cancel();
    assert!(seen.is_cancelled());
}

#[test]
fn scopes_say_where_a_source_takes_part() {
    assert!(Scopes::COMMANDS.contains(Scope::Mixed));
    assert!(Scopes::COMMANDS.contains(Scope::Commands));
    assert!(!Scopes::COMMANDS.contains(Scope::Files));
    assert!(Scopes::FILES.contains(Scope::Files));
    assert!(!Scopes::FILES.contains(Scope::Commands));
    assert!(Scopes::new(&[Scope::Files]).contains(Scope::Files));
    assert!(!Scopes::new(&[Scope::Files]).contains(Scope::Mixed));
    for scope in [Scope::Mixed, Scope::Commands, Scope::Files] {
        assert!(Scopes::ALL.contains(scope));
    }
}

#[test]
fn groups_sort_commands_folders_files_then_modules() {
    let mut groups = [
        Group::Module("b"),
        Group::Files,
        Group::Module("a"),
        Group::Commands,
        Group::Folders,
    ];
    groups.sort();
    assert_eq!(
        groups,
        [
            Group::Commands,
            Group::Folders,
            Group::Files,
            Group::Module("a"),
            Group::Module("b"),
        ]
    );
}

#[test]
fn a_boxed_source_streams_hits_into_the_sink() {
    let sources: Vec<Box<dyn ResultSource>> = vec![Box::new(Fixed)];
    let sink = HitSink::new(2);
    for source in &sources {
        assert_eq!(source.id(), "fixed");
        assert_eq!(source.group(), Group::Module("Fixed"));
        source.search(&request(2), &sink, &Cancel::new());
    }
    assert_eq!(titles(&sink.drain(2)), ["hit 0", "hit 1"]);
}

#[test]
fn a_cancelled_source_stops_without_hits() {
    let cancel = Cancel::new();
    cancel.cancel();
    let sink = HitSink::new(1);
    Fixed.search(&request(1), &sink, &cancel);
    assert!(sink.drain(1).is_empty());
}

#[test]
fn a_stale_source_run_leaves_the_new_generation_clean() {
    let sink = HitSink::new(1);
    sink.begin(2);
    Fixed.search(&request(1), &sink, &Cancel::new());
    assert!(sink.drain(2).is_empty());
}

#[test]
fn source_and_sink_cross_threads() {
    fn is_send_sync<T: Send + Sync>() {}
    is_send_sync::<HitSink>();
    is_send_sync::<Cancel>();
    is_send_sync::<Arc<dyn ResultSource>>();
}

#[test]
fn targets_cover_every_row_action() {
    let folder = Location::new(r"C:\Users").expect("a valid location");
    let targets = [
        Target::Action("pane.refresh"),
        Target::Core(Command::PaneRefresh),
        Target::Arg("pane.go_to", ArgKind::Path),
        Target::Open {
            location: folder.clone(),
            folder: true,
        },
        Target::GoTo(folder),
        Target::ShowAll(Group::Folders),
    ];
    assert_eq!(targets.len(), 6);
    assert_ne!(targets[0], targets[1]);
}

#[test]
fn a_request_carries_the_parsed_query_and_the_active_folder() {
    let here = Location::new("/home/me").expect("a valid location");
    let request = Request {
        query: Query::parse("/main ext:rs"),
        generation: 9,
        here: Some(here.clone()),
        limit: 500,
    };
    assert_eq!(request.query.scope, Scope::Files);
    assert_eq!(request.here, Some(here));
}
