use std::sync::Arc;
use std::time::{Duration, Instant};

use rusty_core::{FsError, FsErrorKind, Location, ProviderId, ProviderPath};
use rusty_fs::{CancelToken, Capabilities, FakeFs, FakeOp, Fault, FsEntry, ListSink, Provider};

use crate::command_center::file_sources::{Env, split};
use crate::command_center::{
    Cancel, Group, Hit, HitSink, Offline, PathsSource, Query, Request, ResultSource, Results, Row,
    Scope, Target, completion,
};

fn tree() -> Arc<FakeFs> {
    let fs = Arc::new(FakeFs::new().with_batch_size(2));
    fs.add_folder("home/work/projects");
    fs.add_folder("home/work/photos");
    fs.add_folder("home/work/notes");
    fs.add_file("home/work/prices.txt", 1);
    fs
}

fn env(name: &str) -> Option<String> {
    match name {
        "HOME" => Some("/home".to_owned()),
        _ => None,
    }
}

fn at(fs: &FakeFs, path: &str) -> Location {
    Location::with_provider(fs.id(), ProviderPath::from_text(path)).expect("location")
}

fn request(input: &str, here: &Location) -> Request {
    Request {
        query: Query::parse(input),
        generation: 1,
        here: Some(here.clone()),
        limit: 100,
    }
}

fn run(source: &PathsSource, input: &str, here: &Location) -> (Vec<Hit>, Option<Offline>) {
    let request = request(input, here);
    let sink = HitSink::new(1);
    source.search(&request, &sink, &Cancel::new());
    (sink.drain(1), source.offline(&request))
}

fn source(fs: &Arc<FakeFs>) -> PathsSource {
    PathsSource::with_wait(fs.clone(), Arc::new(env), Duration::from_millis(150))
}

fn children(hits: &[Hit]) -> Vec<String> {
    let mut names: Vec<String> = hits
        .iter()
        .filter(|hit| matches!(hit.target, Target::Open { .. }))
        .map(|hit| hit.title.to_string())
        .collect();
    names.sort();
    names
}

#[test]
fn an_existing_folder_gets_a_go_to_row() {
    let fs = tree();
    let here = fs.location("home");
    let (hits, _) = run(&source(&fs), "~/work/projects", &here);
    let go = hits
        .iter()
        .find(|hit| matches!(hit.target, Target::GoTo(_)))
        .expect("go to row");
    assert_eq!(go.target, Target::GoTo(at(&fs, "/home/work/projects")));
    assert_eq!(go.group, Group::Folders);
    assert!(go.title.contains("projects"));
}

#[test]
fn a_missing_folder_gets_no_go_to_row() {
    let fs = tree();
    let (hits, offline) = run(&source(&fs), "~/work/proj", &fs.location("home"));
    assert!(!hits.iter().any(|hit| matches!(hit.target, Target::GoTo(_))));
    assert!(offline.is_none());
}

#[test]
fn children_are_the_folders_of_the_parent_filtered_by_the_last_segment() {
    let fs = tree();
    let here = fs.location("home");
    let (hits, _) = run(&source(&fs), "~/work/p", &here);
    assert_eq!(children(&hits), ["photos", "projects"]);
    let (all, _) = run(&source(&fs), "~/work/", &here);
    assert_eq!(children(&all), ["notes", "photos", "projects"]);
    let go = all.iter().find(|hit| matches!(hit.target, Target::GoTo(_)));
    assert_eq!(
        go.map(|hit| hit.target.clone()),
        Some(Target::GoTo(at(&fs, "/home/work")))
    );
}

#[test]
fn a_bare_tilde_lists_the_home_folder() {
    let fs = tree();
    let (hits, _) = run(&source(&fs), "~", &fs.location("elsewhere"));
    assert_eq!(children(&hits), ["work"]);
    assert!(
        hits.iter()
            .any(|hit| hit.target == Target::GoTo(at(&fs, "/home")))
    );
}

#[test]
fn drive_root_and_relative_forms_split_into_parent_and_filter() {
    let windows = Location::new(r"C:\work").unwrap();
    let env: Env = Arc::new(|name| (name == "USERPROFILE").then(|| r"C:\Users\admin".to_owned()));
    let split = |input: &str| split(input, &windows, &env);
    let root = split("C:").expect("drive root");
    assert_eq!((root.parent.as_str(), root.filter.as_str()), (r"C:\", ""));
    let deep = split(r"C:\Users\ad").expect("deep");
    assert_eq!(
        (deep.parent.as_str(), deep.filter.as_str()),
        (r"C:\Users", "ad")
    );
    let dots = split(r"C:\Users\..").expect("dots");
    assert_eq!((dots.parent.as_str(), dots.filter.as_str()), (r"C:\", ""));
    let home = split("~/docs/").expect("home");
    assert_eq!(
        (home.parent.as_str(), home.filter.as_str()),
        (r"C:\Users\admin\docs", "")
    );
}

#[test]
fn tab_completes_the_selected_folder_name_with_a_separator() {
    let fs = tree();
    let (hits, _) = run(&source(&fs), "~/work/pr", &fs.location("home"));
    let hit = hits
        .iter()
        .find(|hit| hit.title == "projects")
        .expect("projects");
    assert_eq!(
        completion("~/work/pr", hit).as_deref(),
        Some("~/work/projects/")
    );
    assert_eq!(completion("~", hit).as_deref(), Some("~/projects/"));
    assert_eq!(
        completion(r"C:\a\pr", hit).as_deref(),
        Some(r"C:\a\projects\")
    );
    assert_eq!(completion("C:", hit).as_deref(), Some(r"C:\projects\"));
    let go = hits
        .iter()
        .find(|hit| matches!(hit.target, Target::GoTo(_)));
    assert!(go.is_none_or(|go| completion("~/work/pr", go).is_none()));
}

#[test]
fn an_unreachable_parent_gives_the_note_and_no_rows() {
    let fs = tree();
    fs.fault(
        "home/work",
        FakeOp::List,
        1,
        Fault::Fail(FsErrorKind::NetworkLost),
    );
    let (hits, offline) = run(&source(&fs), "~/work/p", &fs.location("home"));
    assert!(hits.is_empty());
    let note = offline.expect("note");
    assert!(note.path.contains("work"));
    let rows = Results::with_offline(Results::merge(hits, Scope::Files), Some(note));
    assert!(matches!(rows.as_slice(), [Row::Header(_)]));
}

#[test]
fn a_missing_parent_is_quiet() {
    let fs = tree();
    let (hits, offline) = run(&source(&fs), "~/nothing/p", &fs.location("home"));
    assert!(hits.is_empty() && offline.is_none());
}

struct Hangs(Arc<FakeFs>);

impl Provider for Hangs {
    fn id(&self) -> ProviderId {
        self.0.id()
    }

    fn capabilities(&self) -> Capabilities {
        self.0.capabilities()
    }

    fn list(
        &self,
        folder: &Location,
        _sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        while !cancel.is_cancelled() {
            std::thread::sleep(Duration::from_millis(5));
        }
        Err(FsError {
            location: Some(folder.clone()),
            ..FsError::new(FsErrorKind::Cancelled)
        })
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.0.metadata(location)
    }
}

#[test]
fn a_listing_that_never_answers_ends_in_the_note_not_a_hang() {
    let fs = tree();
    let source = PathsSource::with_wait(
        Arc::new(Hangs(fs.clone())),
        Arc::new(env),
        Duration::from_millis(100),
    );
    let started = Instant::now();
    let (hits, offline) = run(&source, "~/work/p", &fs.location("home"));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(hits.is_empty());
    assert!(offline.is_some());
}

#[test]
fn a_new_keystroke_ends_a_pending_listing_at_once() {
    let fs = tree();
    let source = PathsSource::with_wait(
        Arc::new(Hangs(fs.clone())),
        Arc::new(env),
        Duration::from_secs(30),
    );
    let (request, sink, cancel) = (
        request("~/work/p", &fs.location("home")),
        HitSink::new(1),
        Cancel::new(),
    );
    let stop = cancel.clone();
    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(60));
        stop.cancel();
    });
    let started = Instant::now();
    source.search(&request, &sink, &cancel);
    canceller.join().expect("thread");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(source.offline(&request).is_none());
}

#[test]
fn a_path_hit_is_dropped_when_a_tab_or_recent_row_shows_the_same_place() {
    let fs = tree();
    let (mut hits, _) = run(&source(&fs), "~/work/proj", &fs.location("home"));
    let place = at(&fs, "/home/work/projects");
    let mut recent = hits
        .iter()
        .find(|hit| hit.title == "projects")
        .cloned()
        .expect("hit");
    recent.source = "recent";
    recent.score = 5;
    hits.push(recent);
    let rows = Results::merge(hits, Scope::Files);
    let shown: Vec<_> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Hit(hit) | Row::TopHit(hit) => Some(hit),
            _ => None,
        })
        .filter(|hit| matches!(&hit.target, Target::Open { location, .. } if location.same_place(&place)))
        .collect();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].source, "recent");
}
