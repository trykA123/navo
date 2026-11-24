//! Each tab's folder listing runs under its own queue owner: leaving a pending listing cancels
//! that job only, never another pane's listing.

use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use navo_core::Direction;

use super::*;

/// A provider whose `docs` and `music` listings block until cancelled or released (5 s cap).
struct Held {
    fs: Arc<FakeFs>,
    started: Mutex<Vec<String>>,
    cancelled: Mutex<Vec<String>>,
    released: AtomicBool,
}

impl Held {
    /// Wraps the fake tree with nothing started yet.
    fn new(fs: &Arc<FakeFs>) -> Arc<Self> {
        Arc::new(Self {
            fs: Arc::clone(fs),
            started: Mutex::new(Vec::new()),
            cancelled: Mutex::new(Vec::new()),
            released: AtomicBool::new(false),
        })
    }

    /// True once a held listing of a folder whose path ends in `name` has started.
    fn has_started(&self, name: &str) -> bool {
        self.started.lock().unwrap().iter().any(|n| n == name)
    }

    /// Blocks a listing of `name` until release (false) or cancel (true, recorded); 5 s cap.
    fn hold(&self, name: &str, cancel: &CancelToken) -> bool {
        self.started.lock().unwrap().push(name.to_owned());
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.released.load(Ordering::Acquire) && Instant::now() < deadline {
            if cancel.is_cancelled() {
                self.cancelled.lock().unwrap().push(name.to_owned());
                return true;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        false
    }

    /// True once a held listing of `name` saw its cancel token.
    fn was_cancelled(&self, name: &str) -> bool {
        self.cancelled.lock().unwrap().iter().any(|n| n == name)
    }
}

impl Provider for Held {
    fn id(&self) -> navo_core::ProviderId {
        self.fs.id()
    }

    fn capabilities(&self) -> navo_fs::Capabilities {
        self.fs.capabilities()
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn navo_fs::ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        let name = folder
            .as_str()
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_owned();
        if (name == "docs" || name == "music") && self.hold(&name, cancel) {
            return Err(FsError::new(FsErrorKind::Cancelled));
        }
        self.fs.list(folder, sink, cancel)
    }

    fn metadata(&self, location: &Location) -> Result<navo_fs::FsEntry, FsError> {
        self.fs.metadata(location)
    }
}

impl ProviderOps for Held {
    fn create_folder(&self, location: &Location) -> Result<(), FsError> {
        self.fs.create_folder(location)
    }

    fn create_file(&self, location: &Location) -> Result<(), FsError> {
        self.fs.create_file(location)
    }

    fn copy_file(
        &self,
        source: &Location,
        target: &Location,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<u64, FsError> {
        self.fs.copy_file(source, target, progress, cancel)
    }

    fn rename(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.fs.rename(source, target)
    }

    fn replace(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.fs.replace(source, target)
    }

    fn remove_file(&self, location: &Location) -> Result<(), FsError> {
        self.fs.remove_file(location)
    }

    fn remove_link(&self, location: &Location) -> Result<(), FsError> {
        self.fs.remove_link(location)
    }

    fn remove_empty_folder(&self, location: &Location) -> Result<(), FsError> {
        self.fs.remove_empty_folder(location)
    }

    fn recycle(&self, location: &Location) -> Result<(), FsError> {
        self.fs.recycle(location)
    }
}

/// A settled browser at `home` over the held provider.
fn held_browser(fs: &Arc<FakeFs>, held: &Arc<Held>) -> Browser {
    let provider: Arc<dyn ProviderOps> = held.clone();
    let mut browser = Browser::new(
        AppState::new(fs.location("home")),
        provider,
        Arc::new(|| {}),
    );
    browser.start();
    settle(&mut browser);
    browser
}

/// Polls the browser until `done` holds or `limit` passes; returns whether it held.
fn wait_until(browser: &mut Browser, limit: Duration, done: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    while !done() && Instant::now() < deadline {
        browser.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    done()
}

#[test]
fn leaving_a_running_listing_cancels_its_job_within_a_second() {
    let fs = fake();
    let held = Held::new(&fs);
    let mut browser = held_browser(&fs, &held);
    let docs = fs.location("home/docs");
    browser
        .command(&Command::PaneNavigate { location: docs })
        .unwrap();
    assert!(wait_until(&mut browser, Duration::from_secs(2), || held
        .has_started("docs")));
    browser.command(&Command::PaneBack).unwrap();
    let cancelled = wait_until(&mut browser, Duration::from_secs(1), || {
        held.was_cancelled("docs")
    });
    held.released.store(true, Ordering::Release);
    settle(&mut browser);
    assert!(cancelled, "the left listing's job saw no cancel");
    assert!(
        !browser.loading(),
        "a cancelled listing left the browser busy"
    );
    assert_eq!(here(&browser), fs.location("home").as_str());
    assert!(browser.error(browser.state().active_tab().id()).is_none());
}

#[test]
fn leaving_a_queued_listing_drops_that_job_and_spares_the_other_pane() {
    let fs = fake();
    let held = Held::new(&fs);
    let mut browser = held_browser(&fs, &held);
    browser
        .command(&Command::LayoutSplit {
            direction: Direction::Right,
        })
        .unwrap();
    settle(&mut browser);
    let music = fs.location("home/music");
    browser
        .command(&Command::PaneNavigate { location: music })
        .unwrap();
    assert!(wait_until(&mut browser, Duration::from_secs(2), || held
        .has_started("music")));
    browser
        .command(&Command::LayoutFocus {
            direction: Direction::Left,
        })
        .unwrap();
    let docs = fs.location("home/docs");
    browser
        .command(&Command::PaneNavigate { location: docs })
        .unwrap();
    browser.command(&Command::PaneBack).unwrap();
    held.released.store(true, Ordering::Release);
    settle(&mut browser);
    assert!(
        !held.was_cancelled("music"),
        "the other pane's listing was cancelled"
    );
    assert!(
        !held.has_started("docs"),
        "the left pane's queued listing still ran"
    );
    assert!(
        !browser.loading(),
        "a dropped listing left the browser busy"
    );
    assert_eq!(here(&browser), fs.location("home").as_str());
}

#[test]
fn leaving_a_listing_that_adopted_a_running_prefetch_cancels_it() {
    let fs = fake();
    let held = Held::new(&fs);
    let mut browser = held_browser(&fs, &held);
    let row = names(&browser).iter().position(|n| n == "docs").unwrap();
    let to = CursorMove::Row(row as u64);
    browser.command(&Command::PaneCursorMove { to }).unwrap();
    let docs = browser.cursor_folder().unwrap();
    assert!(
        browser.prefetch(docs),
        "the rested folder was not prefetched"
    );
    assert!(wait_until(&mut browser, Duration::from_secs(2), || held
        .has_started("docs")));
    assert!(browser.open_row(row as u64));
    let target = browser.cursor_folder();
    browser.leave_prefetches(target.as_ref());
    browser.command(&Command::PaneBack).unwrap();
    let cancelled = wait_until(&mut browser, Duration::from_secs(1), || {
        held.was_cancelled("docs")
    });
    held.released.store(true, Ordering::Release);
    settle(&mut browser);
    assert!(cancelled, "the adopted prefetch saw no cancel");
    assert!(
        !browser.loading(),
        "the adopted listing left the browser busy"
    );
    assert_eq!(here(&browser), fs.location("home").as_str());
}

#[test]
fn leaving_a_listing_that_adopted_a_queued_prefetch_leaves_the_browser_idle() {
    let fs = fake();
    let held = Held::new(&fs);
    let mut browser = held_browser(&fs, &held);
    let rows = names(&browser);
    let row_of = |name: &str| rows.iter().position(|n| n == name).unwrap() as u64;
    for name in ["music", "docs"] {
        let to = CursorMove::Row(row_of(name));
        browser.command(&Command::PaneCursorMove { to }).unwrap();
        let folder = browser.cursor_folder().unwrap();
        assert!(browser.prefetch(folder), "{name} was not prefetched");
    }
    assert!(wait_until(&mut browser, Duration::from_secs(2), || held
        .has_started("music")));
    assert!(browser.open_row(row_of("docs")));
    browser.command(&Command::PaneBack).unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while browser.loading() && Instant::now() < deadline {
        browser.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    let idle = !browser.loading();
    held.released.store(true, Ordering::Release);
    settle(&mut browser);
    assert!(idle, "the dropped adopted prefetch left the browser busy");
    assert!(
        !held.has_started("docs"),
        "the adopted queued prefetch still ran"
    );
    assert_eq!(here(&browser), fs.location("home").as_str());
}
