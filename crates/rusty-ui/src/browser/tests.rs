use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use rusty_core::{AppState, Command, CursorMove, FsErrorKind, Location};
use rusty_fs::{CancelToken, FakeFs, FakeOp, Fault, Provider, ProviderOps};

use super::*;

fn fake() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    fs.add_folder("home/music");
    fs.add_file("home/a.txt", 3);
    fs.add_file("home/docs/report.txt", 7);
    Arc::new(fs)
}

fn browser_at(fs: &Arc<FakeFs>, path: &str) -> Browser {
    let state = AppState::new(fs.location(path));
    let provider: Arc<dyn ProviderOps> = fs.clone();
    let mut browser = Browser::new(state, provider, Arc::new(|| {}));
    browser.start();
    browser
}

fn settle(browser: &mut Browser) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while browser.loading() && Instant::now() < deadline {
        browser.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    browser.poll();
}

fn names(browser: &Browser) -> Vec<String> {
    browser
        .state()
        .active_tab()
        .listing()
        .map(|l| l.entries().iter().map(|e| e.name.clone()).collect())
        .unwrap_or_default()
}

fn here(browser: &Browser) -> String {
    browser.state().active_tab().location().as_str().to_owned()
}

#[test]
fn start_lists_the_active_tab() {
    let fs = fake();
    let mut browser = browser_at(&fs, "home");
    settle(&mut browser);
    assert_eq!(names(&browser), ["docs", "music", "a.txt"]);
}

#[test]
fn details_line_up_with_the_listing() {
    let fs = fake();
    let mut browser = browser_at(&fs, "home");
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    let sizes: Vec<u64> = browser.details(tab).iter().map(|e| e.size).collect();
    assert_eq!(sizes, [0, 0, 3]);
}

#[test]
fn a_command_runs_its_listing_effect() {
    let fs = fake();
    let mut browser = browser_at(&fs, "home");
    settle(&mut browser);
    let location = fs.location("home/docs");
    browser
        .command(&Command::PaneNavigate { location })
        .unwrap();
    settle(&mut browser);
    assert_eq!(names(&browser), ["report.txt"]);
}

#[test]
fn opening_a_folder_row_enters_it() {
    let fs = fake();
    let mut browser = browser_at(&fs, "home");
    settle(&mut browser);
    assert!(browser.open_row(0));
    settle(&mut browser);
    assert_eq!(here(&browser), fs.location("home/docs").as_str());
}

#[test]
fn opening_a_file_row_launches_it_and_stays() {
    let fs = fake();
    let launched = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = Arc::clone(&launched);
    let mut browser = browser_at(&fs, "home");
    browser.set_launcher(Arc::new(move |location: &Location| {
        seen.lock().unwrap().push(location.clone());
    }));
    settle(&mut browser);
    assert!(browser.open_row(2));
    assert_eq!(here(&browser), fs.location("home").as_str());
    assert_eq!(*launched.lock().unwrap(), [fs.location("home/a.txt")]);
}

#[test]
fn going_up_puts_the_cursor_on_the_folder_left() {
    let fs = fake();
    let mut browser = browser_at(&fs, "home/music");
    settle(&mut browser);
    assert!(browser.go_up());
    settle(&mut browser);
    assert_eq!(here(&browser), fs.location("home").as_str());
    assert_eq!(browser.state().active_tab().view().cursor, 1);
}

#[test]
fn going_up_from_the_root_does_nothing() {
    let fs = fake();
    let mut browser = browser_at(&fs, "");
    settle(&mut browser);
    assert!(!browser.go_up());
}

#[test]
fn a_failed_listing_keeps_its_error_and_an_empty_list() {
    let fs = fake();
    fs.fault(
        "home",
        FakeOp::List,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let mut browser = browser_at(&fs, "home");
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    assert_eq!(
        browser.error(tab).map(|e| e.kind),
        Some(FsErrorKind::AccessDenied)
    );
    assert_eq!(
        browser.state().active_tab().listing().map(|l| l.len()),
        Some(0)
    );
}

#[test]
fn a_success_clears_an_earlier_error() {
    let fs = fake();
    fs.fault(
        "home",
        FakeOp::List,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let mut browser = browser_at(&fs, "home");
    settle(&mut browser);
    browser.command(&Command::PaneRefresh).unwrap();
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    assert!(browser.error(tab).is_none());
}

#[test]
fn a_finished_load_wakes_the_ui() {
    let fs = fake();
    let woken = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&woken);
    let provider: Arc<dyn ProviderOps> = fs.clone();
    let mut browser = Browser::new(
        AppState::new(fs.location("home")),
        provider,
        Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }),
    );
    browser.start();
    settle(&mut browser);
    assert!(woken.load(Ordering::SeqCst) >= 1);
}

#[test]
fn a_late_answer_for_an_old_request_is_ignored() {
    let fs = fake();
    let mut browser = browser_at(&fs, "home");
    let docs = fs.location("home/docs");
    browser
        .command(&Command::PaneNavigate { location: docs })
        .unwrap();
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    assert_eq!(names(&browser), ["report.txt"]);
    assert_eq!(browser.details(tab).len(), 1);
}

#[test]
fn save_session_is_requested_once() {
    let fs = fake();
    let mut browser = browser_at(&fs, "home");
    browser.command(&Command::AppSaveSession).unwrap();
    assert!(browser.take_save_request());
    assert!(!browser.take_save_request());
}

#[test]
fn cursor_commands_reach_the_state() {
    let fs = fake();
    let mut browser = browser_at(&fs, "home");
    settle(&mut browser);
    browser
        .command(&Command::PaneCursorMove {
            to: CursorMove::Last,
        })
        .unwrap();
    assert_eq!(browser.state().active_tab().view().cursor, 2);
}

#[test]
fn locations_from_other_providers_are_listed_by_the_given_provider() {
    let fs = fake();
    let mut browser = browser_at(&fs, "home");
    settle(&mut browser);
    let bad = Location::new("/not/fake").unwrap();
    browser
        .command(&Command::PaneNavigate { location: bad })
        .unwrap();
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    assert_eq!(
        browser.error(tab).map(|e| e.kind),
        Some(FsErrorKind::NotFound)
    );
}

#[test]
fn details_are_empty_while_a_new_folder_loads() {
    let fs = fake();
    let mut browser = browser_at(&fs, "home");
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    let docs = fs.location("home/docs");
    browser
        .command(&Command::PaneNavigate { location: docs })
        .unwrap();
    assert!(browser.details(tab).is_empty());
}

#[test]
fn an_old_error_is_hidden_after_leaving_the_folder() {
    let fs = fake();
    fs.fault(
        "home",
        FakeOp::List,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let mut browser = browser_at(&fs, "home");
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    let docs = fs.location("home/docs");
    browser
        .command(&Command::PaneNavigate { location: docs })
        .unwrap();
    assert!(browser.error(tab).is_none());
}

#[test]
fn opening_a_link_row_enters_it() {
    let fs = fake();
    fs.add_entry(
        "home",
        rusty_fs::FsEntry {
            name: rusty_core::ProviderPath::from_text("aa-link"),
            kind: rusty_fs::EntryKind::Link,
            size: 0,
            modified_ms: None,
            attributes: rusty_fs::Attributes {
                link_to_folder: true,
                ..rusty_fs::Attributes::default()
            },
        },
    );
    let mut browser = browser_at(&fs, "home");
    settle(&mut browser);
    assert_eq!(names(&browser), ["docs", "music", "a.txt", "aa-link"]);
    assert!(browser.open_row(3));
    assert_eq!(here(&browser), fs.location("home/aa-link").as_str());
}

#[test]
fn marking_a_folder_measures_its_size() {
    let fs = fake();
    let mut browser = browser_at(&fs, "home");
    settle(&mut browser);
    browser.command(&Command::SelectionToggle).unwrap();
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    assert_eq!(browser.details(tab)[0].size, 7);
}

#[test]
fn marking_a_folder_does_not_measure_it_when_measuring_is_off() {
    let fs = fake();
    let mut browser = browser_at(&fs, "home");
    settle(&mut browser);
    browser.set_space_measures_folders(false);
    browser.command(&Command::SelectionToggle).unwrap();
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    assert_ne!(browser.details(tab)[0].size, 7);
}

#[test]
fn a_size_for_a_folder_left_behind_is_ignored() {
    let fs = fake();
    let mut browser = browser_at(&fs, "home");
    settle(&mut browser);
    browser.command(&Command::SelectionToggle).unwrap();
    let music = fs.location("home/music");
    browser
        .command(&Command::PaneNavigate { location: music })
        .unwrap();
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    assert!(browser.details(tab).is_empty());
}

#[test]
fn a_folder_marked_then_unmarked_keeps_its_size() {
    let fs = fake();
    let mut browser = browser_at(&fs, "home");
    settle(&mut browser);
    browser.command(&Command::SelectionToggle).unwrap();
    browser.command(&Command::SelectionToggle).unwrap();
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    assert_eq!(browser.details(tab)[0].size, 7);
}

struct Gated {
    fs: Arc<FakeFs>,
    open: std::sync::atomic::AtomicBool,
}

impl Provider for Gated {
    fn id(&self) -> rusty_core::ProviderId {
        self.fs.id()
    }

    fn capabilities(&self) -> rusty_fs::Capabilities {
        self.fs.capabilities()
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn rusty_fs::ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while folder.as_str().ends_with("docs")
            && !self.open.load(std::sync::atomic::Ordering::Acquire)
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(1));
        }
        self.fs.list(folder, sink, cancel)
    }

    fn metadata(&self, location: &Location) -> Result<rusty_fs::FsEntry, FsError> {
        self.fs.metadata(location)
    }
}

impl ProviderOps for Gated {
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

#[test]
fn a_running_folder_size_does_not_hold_up_the_next_listing() {
    let fs = fake();
    let gated = Arc::new(Gated {
        fs: fs.clone(),
        open: std::sync::atomic::AtomicBool::new(false),
    });
    let provider: Arc<dyn ProviderOps> = gated.clone();
    let mut browser = Browser::new(
        AppState::new(fs.location("home")),
        provider,
        Arc::new(|| {}),
    );
    browser.start();
    settle(&mut browser);
    browser.command(&Command::SelectionToggle).unwrap();
    let music = fs.location("home/music");
    browser
        .command(&Command::PaneNavigate { location: music })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while browser.state().active_tab().listing().is_none() && Instant::now() < deadline {
        browser.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    let listed = browser.state().active_tab().listing().is_some();
    gated.open.store(true, std::sync::atomic::Ordering::Release);
    settle(&mut browser);
    assert!(listed);
}

#[test]
fn a_link_to_a_file_is_not_opened_as_a_folder() {
    let fs = fake();
    fs.add_entry(
        "home",
        rusty_fs::FsEntry {
            name: rusty_core::ProviderPath::from_text("aa-link"),
            kind: rusty_fs::EntryKind::Link,
            size: 0,
            modified_ms: None,
            attributes: rusty_fs::Attributes::default(),
        },
    );
    let mut browser = browser_at(&fs, "home");
    settle(&mut browser);
    assert!(browser.open_row(3));
    assert_eq!(here(&browser), fs.location("home").as_str());
}

mod rename_batch;
