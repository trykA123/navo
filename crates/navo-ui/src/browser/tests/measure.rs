use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver, Sender};

use super::*;

const WAIT: Duration = Duration::from_secs(5);

pub(super) struct Walk {
    cancel: CancelToken,
    release: Sender<bool>,
    observed: Receiver<bool>,
}

impl Walk {
    fn finish(self, wait_cancelled: bool) -> bool {
        self.release.send(wait_cancelled).unwrap();
        self.observed.recv_timeout(WAIT).unwrap()
    }

    fn crash(self) {
        drop(self.release);
    }
}

pub(super) fn gate_walk(cancel: &CancelToken, walks: &Sender<Walk>) {
    let (release, gate) = mpsc::channel();
    let (observed, result) = mpsc::channel();
    walks
        .send(Walk {
            cancel: cancel.clone(),
            release,
            observed: result,
        })
        .unwrap();
    match gate.recv_timeout(WAIT) {
        Ok(true) => {
            let _ = cancel.wait_cancelled(WAIT);
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => panic!("size walk crashed on purpose"),
        Ok(false) | Err(mpsc::RecvTimeoutError::Timeout) => {}
    }
    let _ = observed.send(cancel.is_cancelled());
}

fn measuring() -> (Browser, Arc<FakeFs>, Receiver<Walk>, Receiver<()>) {
    let fs = fake();
    let (walks, started) = mpsc::channel();
    let (wake, woken) = mpsc::channel();
    let provider = Arc::new(Gated {
        fs: fs.clone(),
        open: AtomicBool::new(false),
        walks: Some(walks),
    });
    let mut browser = Browser::new(
        AppState::new(fs.location("home")),
        provider,
        Arc::new(move || {
            let _ = wake.send(());
        }),
    );
    browser.start();
    settle(&mut browser);
    while woken.try_recv().is_ok() {}
    browser.command(&Command::SelectionToggle).unwrap();
    (browser, fs, started, woken)
}

fn navigate(browser: &mut Browser, fs: &FakeFs, path: &str) {
    browser
        .command(&Command::PaneNavigate {
            location: fs.location(path),
        })
        .unwrap();
}

#[test]
fn leaving_a_folder_cancels_its_running_size_walk() {
    let (mut browser, fs, started, _) = measuring();
    let walk = started.recv_timeout(WAIT).unwrap();
    navigate(&mut browser, &fs, "home/music");
    assert!(walk.finish(true));
    settle(&mut browser);
    assert!(!browser.loading());
}

#[test]
fn a_cancelled_size_does_not_show_after_returning_to_the_folder() {
    let (mut browser, fs, started, _) = measuring();
    let walk = started.recv_timeout(WAIT).unwrap();
    navigate(&mut browser, &fs, "home/music");
    navigate(&mut browser, &fs, "home");
    assert!(walk.finish(true));
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    assert_eq!(browser.details(tab)[0].size, 0);
    assert!(!browser.loading());
}

#[test]
fn a_size_delivered_before_leaving_does_not_show_on_a_later_visit() {
    let (mut browser, fs, started, woken) = measuring();
    assert!(!started.recv_timeout(WAIT).unwrap().finish(false));
    woken.recv_timeout(WAIT).unwrap();
    let measured = browser.receiver.recv_timeout(WAIT).unwrap();
    assert!(matches!(measured, Message::Measured(..)));
    navigate(&mut browser, &fs, "home/music");
    navigate(&mut browser, &fs, "home");
    settle(&mut browser);
    browser.sender.send(measured).unwrap();
    browser.poll();
    let tab = browser.state().active_tab().id();
    assert_eq!(browser.details(tab)[0].size, 0);
}

#[test]
fn refreshing_the_same_folder_keeps_its_size_walk() {
    let (mut browser, _, started, _) = measuring();
    let walk = started.recv_timeout(WAIT).unwrap();
    browser.command(&Command::PaneRefresh).unwrap();
    let deadline = Instant::now() + WAIT;
    while browser.state().active_tab().pending_request().is_some() && Instant::now() < deadline {
        browser.poll();
        std::thread::yield_now();
    }
    assert!(browser.state().active_tab().pending_request().is_none());
    assert!(!walk.finish(false));
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    assert_eq!(browser.details(tab)[0].size, 7);
    assert!(!browser.loading());
}

#[test]
fn closing_a_tab_cancels_its_size_walk() {
    let (mut browser, fs, started, _) = measuring();
    let walk = started.recv_timeout(WAIT).unwrap();
    browser
        .command(&Command::PaneOpenTab {
            location: Some(fs.location("home/music")),
        })
        .unwrap();
    browser
        .command(&Command::PaneCloseTab { tab: Some(0) })
        .unwrap();
    assert!(walk.finish(true));
    settle(&mut browser);
    assert!(!browser.loading());
}

#[test]
fn tabs_measuring_at_once_do_not_cancel_each_other() {
    let (mut browser, fs, started, _) = measuring();
    let first = started.recv_timeout(WAIT).unwrap();
    browser.pools.queue(JobKind::Size).set_volume(
        &volume_of(&fs.location("home/docs")),
        navo_fs::VolumeKind::Ssd,
    );
    browser
        .command(&Command::PaneOpenTab { location: None })
        .unwrap();
    let deadline = Instant::now() + WAIT;
    while browser.state().active_tab().listing().is_none() && Instant::now() < deadline {
        browser.poll();
        std::thread::yield_now();
    }
    browser.command(&Command::SelectionToggle).unwrap();
    let second = started.recv_timeout(WAIT).unwrap();
    assert!(!first.cancel.is_cancelled());
    assert!(!second.cancel.is_cancelled());
    browser
        .command(&Command::PaneCloseTab { tab: Some(0) })
        .unwrap();
    assert!(first.finish(true));
    assert!(!second.finish(false));
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    assert_eq!(browser.details(tab)[0].size, 7);
    assert!(!browser.loading());
}

#[test]
fn a_crashed_size_walk_ends_the_spinner_and_shows_no_size() {
    let (mut browser, _, started, _) = measuring();
    started.recv_timeout(WAIT).unwrap().crash();
    settle(&mut browser);
    assert!(!browser.loading());
    let tab = browser.state().active_tab().id();
    assert_eq!(browser.details(tab)[0].size, 0);
}

#[test]
fn measurement_resolves_a_results_folder_under_its_base() {
    use navo_core::ProviderPath;
    use navo_fs::{Attributes, EntryKind, FsEntry};
    let fs = Arc::new(FakeFs::new());
    fs.add_entry(
        "shown",
        FsEntry {
            name: ProviderPath::from_text(r"sub\a.txt"),
            kind: EntryKind::Folder,
            size: 0,
            modified_ms: None,
            attributes: Attributes::default(),
        },
    );
    fs.add_file("base/sub/a.txt/child.bin", 13);
    let mut browser = browser_at(&fs, "shown");
    settle(&mut browser);
    let mut tab = browser.state().active_tab().clone();
    tab.set_base(Some(fs.location("base")));
    browser.measure_entry(&tab, r"sub\a.txt".to_owned());
    settle(&mut browser);
    assert_eq!(browser.details(tab.id())[0].size, 13);
    assert_eq!(fs.calls("base/sub/a.txt", FakeOp::List), 1);
}

fn sized_row(bytes: u64, on_disk: Option<u64>) -> (Browser, TabId, String) {
    let fs = fake();
    let mut browser = browser_at(&fs, "home");
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    let name = browser.state().active_tab().listing().unwrap().entries()[0]
        .name
        .to_string();
    let folder = browser.state().active_tab().location().clone();
    let size = navo_fs::FolderSize {
        bytes,
        on_disk: on_disk.unwrap_or_default(),
        on_disk_known: on_disk.is_some(),
        ..navo_fs::FolderSize::default()
    };
    assert!(browser.show_folder_size(&Measured {
        tab,
        folder,
        name: name.clone(),
        size: Some(size),
    }));
    (browser, tab, name)
}

#[test]
fn a_measured_row_shows_the_number_of_the_current_mode() {
    let (mut browser, tab, _) = sized_row(100, Some(4096));
    assert_eq!(browser.details(tab)[0].size, 100);
    browser.set_size_mode(SizeMode::OnDisk);
    assert_eq!(browser.details(tab)[0].size, 4096);
    browser.set_size_mode(SizeMode::Apparent);
    assert_eq!(browser.details(tab)[0].size, 100);
}

#[test]
fn on_disk_falls_back_to_the_apparent_size_when_unknown() {
    let (mut browser, tab, _) = sized_row(100, None);
    browser.set_size_mode(SizeMode::OnDisk);
    assert_eq!(browser.details(tab)[0].size, 100);
}

#[test]
fn a_size_arriving_in_on_disk_mode_shows_the_on_disk_number() {
    let fs = fake();
    let mut browser = browser_at(&fs, "home");
    browser.set_size_mode(SizeMode::OnDisk);
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    let folder = browser.state().active_tab().location().clone();
    let name = browser.state().active_tab().listing().unwrap().entries()[0]
        .name
        .to_string();
    let size = navo_fs::FolderSize {
        bytes: 10,
        on_disk: 8192,
        on_disk_known: true,
        ..navo_fs::FolderSize::default()
    };
    browser.show_folder_size(&Measured {
        tab,
        folder,
        name,
        size: Some(size),
    });
    assert_eq!(browser.details(tab)[0].size, 8192);
}

#[test]
fn changing_the_mode_keeps_unmeasured_rows_and_starts_no_walk() {
    let (mut browser, tab, _) = sized_row(100, Some(4096));
    let other = browser.details(tab)[1].size;
    browser.set_size_mode(SizeMode::OnDisk);
    assert_eq!(browser.details(tab)[1].size, other);
    assert!(!browser.measuring());
}
