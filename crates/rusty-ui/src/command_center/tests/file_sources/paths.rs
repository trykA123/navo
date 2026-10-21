use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use rusty_core::{FsError, FsErrorKind, Location, ProviderId, ProviderPath};
use rusty_fs::{CancelToken, Capabilities, FakeFs, FakeOp, Fault, FsEntry, ListSink, Provider};

use crate::command_center::file_sources::{Env, split};
use crate::command_center::{
    Cancel, Group, Hit, HitSink, Offline, PathListings, PathsSource, Query, Request, ResultSource,
    Results, Row, Scope, Target, completion,
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

struct Held {
    inner: Arc<FakeFs>,
    is_open: AtomicBool,
    entered: AtomicUsize,
    busy: AtomicUsize,
    most: AtomicUsize,
}

impl Held {
    fn new(inner: Arc<FakeFs>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            is_open: AtomicBool::new(false),
            entered: AtomicUsize::new(0),
            busy: AtomicUsize::new(0),
            most: AtomicUsize::new(0),
        })
    }

    fn count(counter: &AtomicUsize) -> usize {
        counter.load(Ordering::SeqCst)
    }

    fn wait_until(&self, what: &str, done: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(self) {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

impl Provider for Held {
    fn id(&self) -> ProviderId {
        self.inner.id()
    }

    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.entered.fetch_add(1, Ordering::SeqCst);
        let busy = self.busy.fetch_add(1, Ordering::SeqCst) + 1;
        self.most.fetch_max(busy, Ordering::SeqCst);
        while !self.is_open.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(2));
        }
        let result = self.inner.list(folder, sink, cancel);
        self.busy.fetch_sub(1, Ordering::SeqCst);
        result
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.inner.metadata(location)
    }
}

fn keystrokes() -> Vec<String> {
    (0..20)
        .map(|typed| format!("~/work/{}", &"abcdefghijklmnopqrst"[..typed]))
        .collect()
}

struct Clocked {
    listings: PathListings,
    seconds: Arc<AtomicU64>,
}

fn clocked(provider: Arc<dyn Provider>, wait: Duration) -> Clocked {
    let (base, seconds) = (Instant::now(), Arc::new(AtomicU64::new(0)));
    let offset = seconds.clone();
    let now = Arc::new(move || base + Duration::from_secs(offset.load(Ordering::SeqCst)));
    Clocked {
        listings: PathListings::with_clock(provider, wait, now),
        seconds,
    }
}

#[test]
fn twenty_keystrokes_under_a_hung_parent_start_one_listing() {
    let fs = tree();
    let held = Held::new(fs.clone());
    let listings = clocked(held.clone(), Duration::from_millis(50)).listings;
    let source = PathsSource::new(listings.clone(), Arc::new(env));
    for input in keystrokes() {
        let (hits, offline) = run(&source, &input, &fs.location("home"));
        assert!(hits.is_empty(), "{input}");
        assert!(offline.is_some_and(|note| note.path.contains("work")));
    }
    held.wait_until("the listing to start", |held| {
        Held::count(&held.entered) >= 1
    });
    assert_eq!(Held::count(&held.entered), 1);
    assert!(listings.is_unreachable(&at(&fs, "/home/work")));
    held.is_open.store(true, Ordering::SeqCst);
}

#[test]
fn keystrokes_that_cancel_their_wait_share_the_running_listing() {
    let fs = tree();
    let held = Held::new(fs.clone());
    let source = PathsSource::new(
        clocked(held.clone(), Duration::from_secs(30)).listings,
        Arc::new(env),
    );
    let cancel = Cancel::new();
    cancel.cancel();
    for input in keystrokes() {
        let request = request(&input, &fs.location("home"));
        source.search(&request, &HitSink::new(1), &cancel);
    }
    held.is_open.store(true, Ordering::SeqCst);
    let (hits, _) = run(&source, "~/work/p", &fs.location("home"));
    assert_eq!(children(&hits), ["photos", "projects"]);
    assert_eq!(Held::count(&held.entered), 1);
}

#[test]
fn a_new_parent_waits_for_the_hung_one_and_lists_after_its_release() {
    let fs = tree();
    let held = Held::new(fs.clone());
    let listings = clocked(held.clone(), Duration::from_millis(50)).listings;
    let source = PathsSource::new(listings.clone(), Arc::new(env));
    let home = fs.location("home");
    run(&source, "~/work/p", &home);
    let (hits, offline) = run(&source, "~/", &home);
    assert!(hits.is_empty());
    assert!(offline.is_some_and(|note| note.path.contains("work")));
    assert_eq!(Held::count(&held.entered), 1);
    held.is_open.store(true, Ordering::SeqCst);
    held.wait_until("the hung listing to free its flight", |_| {
        !listings.is_unreachable(&at(&fs, "/home/work"))
    });
    let (hits, offline) = run(&source, "~/", &home);
    assert_eq!(children(&hits), ["work"]);
    assert!(offline.is_none());
    let (hits, _) = run(&source, "~/work/p", &home);
    assert_eq!(children(&hits), ["photos", "projects"]);
    assert_eq!(Held::count(&held.entered), 2);
    assert_eq!(Held::count(&held.most), 1);
}

#[test]
fn filter_changes_reuse_the_listing_and_a_reopen_lists_again() {
    let fs = tree();
    let clocked = clocked(fs.clone(), Duration::from_secs(2));
    let source = PathsSource::new(clocked.listings.clone(), Arc::new(env));
    let home = fs.location("home");
    for input in ["~/work/", "~/work/p", "~/work/pr", "~/work/n"] {
        assert!(!run(&source, input, &home).0.is_empty(), "{input}");
    }
    assert_eq!(fs.calls("home/work", FakeOp::List), 1);
    clocked.seconds.store(1, Ordering::SeqCst);
    let reopened = PathsSource::new(clocked.listings, Arc::new(env));
    run(&reopened, "~/work/p", &home);
    assert_eq!(fs.calls("home/work", FakeOp::List), 2);
}

#[test]
fn an_unreachable_parent_is_not_listed_again_for_30_seconds() {
    let fs = tree();
    fs.fault(
        "home/work",
        FakeOp::List,
        1,
        Fault::Fail(FsErrorKind::NetworkLost),
    );
    let clocked = clocked(fs.clone(), Duration::from_secs(2));
    let home = fs.location("home");
    let open = || PathsSource::new(clocked.listings.clone(), Arc::new(env));
    assert!(run(&open(), "~/work/p", &home).1.is_some());
    clocked.seconds.store(29, Ordering::SeqCst);
    let (hits, offline) = run(&open(), "~/work/", &home);
    assert!(hits.is_empty() && offline.is_some());
    assert_eq!(fs.calls("home/work", FakeOp::List), 1);
    clocked.seconds.store(31, Ordering::SeqCst);
    let (hits, offline) = run(&open(), "~/work/p", &home);
    assert_eq!(children(&hits), ["photos", "projects"]);
    assert!(offline.is_none());
    assert_eq!(fs.calls("home/work", FakeOp::List), 2);
}

#[derive(Default)]
struct Shares {
    is_open: AtomicBool,
    entered: AtomicUsize,
}

impl Provider for Shares {
    fn id(&self) -> ProviderId {
        ProviderId::local()
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn list(
        &self,
        folder: &Location,
        _sink: &mut dyn ListSink,
        _cancel: &CancelToken,
    ) -> Result<(), FsError> {
        if folder.as_str().starts_with(r"\\") {
            self.entered.fetch_add(1, Ordering::SeqCst);
            while !self.is_open.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        Ok(())
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        Err(FsError {
            location: Some(location.clone()),
            ..FsError::new(FsErrorKind::NotFound)
        })
    }
}

#[test]
fn a_hung_share_listing_does_not_delay_a_local_drive() {
    let shares = Arc::new(Shares::default());
    let source = PathsSource::with_wait(shares.clone(), Arc::new(env), Duration::from_secs(2));
    let here = Location::new(r"C:\work").unwrap();
    let request = request(r"\\nas\share\", &here);
    let cancel = Cancel::new();
    cancel.cancel();
    source.search(&request, &HitSink::new(1), &cancel);
    let deadline = Instant::now() + Duration::from_secs(10);
    while shares.entered.load(Ordering::SeqCst) == 0 {
        assert!(Instant::now() < deadline, "the share listing never started");
        std::thread::sleep(Duration::from_millis(2));
    }
    let started = Instant::now();
    let (hits, offline) = run(&source, r"C:\", &here);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(offline.is_none());
    assert!(hits.iter().any(|hit| matches!(hit.target, Target::GoTo(_))));
    shares.is_open.store(true, Ordering::SeqCst);
}
