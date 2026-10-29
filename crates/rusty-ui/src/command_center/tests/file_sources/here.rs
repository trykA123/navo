use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rusty_core::{FsError, FsErrorKind, Location, ProviderId};
use rusty_fs::{CancelToken, Capabilities, FakeFs, FakeOp, FsEntry, ListSink, Provider};

use crate::command_center::{
    Cancel, Capped, Group, Heading, HereOptions, HereSource, HereWalks, Hit, HitSink, Query,
    RecentSource, Request, ResultSource, Results, Row, Scope, Target,
};
use crate::window_view::Frecent;

fn tree() -> Arc<FakeFs> {
    let fs = Arc::new(FakeFs::new().with_batch_size(2));
    fs.add_file("work/notes/budget-2024.txt", 10);
    fs.add_file("work/notes/deep/ideas.md", 10);
    fs.add_file("work/src/main.rs", 10);
    fs.add_folder("work/budget-archive");
    fs.add_file("other/readme.txt", 10);
    fs
}

struct Setup {
    fs: Arc<FakeFs>,
    source: HereSource,
    offset: Arc<AtomicU64>,
}

fn setup(fs: Arc<FakeFs>, cap: usize) -> Setup {
    let (base, offset) = (Instant::now(), Arc::new(AtomicU64::new(0)));
    let seconds = offset.clone();
    let options = HereOptions {
        cap,
        max_age: Duration::from_secs(60),
        now: Arc::new(move || base + Duration::from_secs(seconds.load(Ordering::SeqCst))),
    };
    let walks = HereWalks::with_options(fs.clone(), options);
    Setup {
        source: HereSource::new(walks),
        fs,
        offset,
    }
}

fn request(input: &str, here: &Location) -> Request {
    Request {
        query: Query::parse(input),
        generation: 1,
        here: Some(here.clone()),
        limit: 100,
    }
}

fn settled(source: &HereSource, request: &Request) -> (Vec<Hit>, Option<Capped>) {
    let (sink, cancel) = (HitSink::new(request.generation), Cancel::new());
    loop {
        source.search(request, &sink, &cancel);
        let hits = sink.drain(request.generation);
        if !source.wait_for_more(request, &cancel) {
            return (hits, source.capped(request));
        }
    }
}

fn find(setup: &Setup, input: &str, root: &str) -> Vec<Hit> {
    settled(&setup.source, &request(input, &setup.fs.location(root))).0
}

fn titles(hits: &[Hit]) -> Vec<String> {
    let mut titles: Vec<String> = hits.iter().map(|hit| hit.title.to_string()).collect();
    titles.sort();
    titles
}

#[test]
fn finds_nested_files_and_folders_by_name_terms_with_open_targets() {
    let setup = setup(tree(), 1_000);
    let hits = find(&setup, "budget 2024", "work");
    assert_eq!(titles(&hits), ["budget-2024.txt"]);
    let file = &hits[0];
    assert_eq!((file.source, file.group), ("here", Group::Files));
    assert_eq!(file.detail, setup.fs.location("work/notes").as_str());
    assert_eq!(
        file.target,
        Target::Open {
            location: setup.fs.location("work/notes/budget-2024.txt"),
            folder: false
        }
    );
    let hits = find(&setup, "budget", "work");
    let folder = hits
        .iter()
        .find(|hit| hit.title == "budget-archive")
        .unwrap();
    assert_eq!(folder.group, Group::Folders);
    assert!(matches!(&folder.target, Target::Open { folder: true, .. }));
}

#[test]
fn only_the_walked_root_is_searched() {
    let setup = setup(tree(), 1_000);
    assert!(find(&setup, "readme", "work").is_empty());
    assert_eq!(titles(&find(&setup, "readme", "other")), ["readme.txt"]);
}

#[test]
fn the_walk_is_reused_for_a_minute_then_repeated_and_a_new_root_walks_again() {
    let setup = setup(tree(), 1_000);
    let listed = |path: &str| setup.fs.calls(path, FakeOp::List);
    find(&setup, "budget", "work");
    find(&setup, "ideas", "work");
    assert_eq!(listed("work"), 1);
    setup.offset.store(59, Ordering::SeqCst);
    find(&setup, "main", "work");
    assert_eq!(listed("work"), 1);
    find(&setup, "readme", "other");
    assert_eq!((listed("other"), listed("work")), (1, 1));
    find(&setup, "main", "work");
    assert_eq!(listed("work"), 2);
    setup.offset.store(121, Ordering::SeqCst);
    find(&setup, "main", "work");
    assert_eq!(listed("work"), 3);
}

#[test]
fn a_walk_past_the_cap_reports_the_cap_and_the_header_says_so() {
    let capped = setup(tree(), 5);
    let (_, note) = settled(
        &capped.source,
        &request("budget", &capped.fs.location("work")),
    );
    let note = note.expect("cap note");
    assert_eq!(note.cap, 5);
    assert_eq!(note.root, capped.fs.location("work").as_str());
    let rows = Results::with_cap(
        vec![Row::Header(Heading::Group(Group::Files))],
        Some(note.clone()),
    );
    assert_eq!(
        rows,
        [Row::Header(Heading::FilesCapped {
            cap: 5,
            root: note.root
        })]
    );
    let roomy = setup(tree(), 1_000);
    let (_, note) = settled(
        &roomy.source,
        &request("budget", &roomy.fs.location("work")),
    );
    assert_eq!(note, None);
}

struct Gate {
    inner: Arc<FakeFs>,
    is_open: AtomicBool,
    entered: Mutex<usize>,
    saw_cancel: AtomicBool,
}

impl Gate {
    fn new(inner: Arc<FakeFs>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            is_open: AtomicBool::new(false),
            entered: Mutex::new(0),
            saw_cancel: AtomicBool::new(false),
        })
    }

    fn wait_until(&self, what: &str, done: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(self) {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Provider for Gate {
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
        *self.entered.lock().unwrap() += 1;
        while !self.is_open.load(Ordering::SeqCst) {
            if cancel.is_cancelled() {
                self.saw_cancel.store(true, Ordering::SeqCst);
                return Err(FsError::new(FsErrorKind::Cancelled));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        self.inner.list(folder, sink, cancel)
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.inner.metadata(location)
    }
}

#[test]
fn reopening_on_a_hung_root_reuses_the_running_walk() {
    let fs = tree();
    let gate = Gate::new(fs.clone());
    let walks = HereWalks::new(gate.clone());
    let root = fs.location("work");
    let (sink, cancel) = (HitSink::new(1), Cancel::new());
    HereSource::new(walks.clone()).search(&request("budget", &root), &sink, &cancel);
    gate.wait_until("the walk to start", |gate| {
        *gate.entered.lock().unwrap() >= 1
    });
    let reopened = HereSource::new(walks);
    for typed in ["b", "bu", "bud", "budg", "budget"] {
        reopened.search(&request(typed, &root), &sink, &cancel);
    }
    assert_eq!(*gate.entered.lock().unwrap(), 1);
    gate.is_open.store(true, Ordering::SeqCst);
    let (hits, _) = settled(&reopened, &request("budget", &root));
    assert_eq!(titles(&hits), ["budget-2024.txt", "budget-archive"]);
    assert_eq!(fs.calls("work", FakeOp::List), 1);
    assert!(!gate.saw_cancel.load(Ordering::SeqCst));
}

#[test]
fn a_blocked_root_starts_no_walk() {
    let fs = tree();
    let walks = HereWalks::new(fs.clone());
    let source = HereSource::new(walks).skipping(Arc::new(|_| true));
    let request = request("budget", &fs.location("work"));
    let (sink, cancel) = (HitSink::new(1), Cancel::new());
    source.search(&request, &sink, &cancel);
    assert!(sink.drain(1).is_empty());
    assert!(!source.wait_for_more(&request, &cancel));
    assert_eq!(fs.calls("work", FakeOp::List), 0);
}

#[test]
fn a_place_found_by_recent_and_here_shows_once_as_the_recent_row() {
    let setup = setup(tree(), 1_000);
    let file = setup.fs.location("work/notes/budget-2024.txt");
    let known = file.as_str().to_owned();
    let recent = RecentSource::new(Arc::new(move |_| {
        vec![Frecent {
            location: known.clone(),
            is_folder: false,
            weight: 5.0,
        }]
    }));
    let root = setup.fs.location("work");
    let request = request("budget", &root);
    let (sink, cancel) = (HitSink::new(1), Cancel::new());
    recent.search(&request, &sink, &cancel);
    let mut hits = sink.drain(1);
    hits.extend(settled(&setup.source, &request).0);
    let rows = Results::merge(hits, Scope::Files);
    let shown: Vec<&Hit> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Hit(hit) => Some(hit),
            _ => None,
        })
        .collect();
    let of_file: Vec<&&Hit> = shown
        .iter()
        .filter(|hit| hit.title == "budget-2024.txt")
        .collect();
    assert_eq!(of_file.len(), 1, "{rows:?}");
    assert_eq!(of_file[0].source, "recent");
    assert!(shown.iter().any(|hit| hit.source == "here"));
}

#[test]
fn hits_arrive_in_a_later_round_while_the_walk_grows() {
    let fs = tree();
    let gate = Gate::new(fs.clone());
    let source = HereSource::new(HereWalks::new(gate.clone()));
    let request = request("budget", &fs.location("work"));
    let (sink, cancel) = (HitSink::new(1), Cancel::new());
    source.search(&request, &sink, &cancel);
    assert!(sink.drain(1).is_empty());
    gate.is_open.store(true, Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_secs(10);
    let (mut rounds, mut hits) = (0, Vec::new());
    while source.wait_for_more(&request, &cancel) {
        assert!(Instant::now() < deadline, "the walk did not finish");
        rounds += 1;
        source.search(&request, &sink, &cancel);
        hits = sink.drain(1);
    }
    assert!(rounds > 0);
    assert_eq!(hits.len(), 2);
}
