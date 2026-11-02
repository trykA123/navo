use navo_fs::VolumeKind;

use std::sync::mpsc;

use super::measure::Walk;
use super::*;
use crate::file_list::{SortBy, SortKey};

fn kinds(kind: VolumeKind) -> navo_fs::Classifier {
    Arc::new(move |_| kind)
}

fn many_folders(count: usize) -> Arc<FakeFs> {
    let fs = FakeFs::new();
    for n in 0..count {
        fs.add_file(&format!("many/f{n:04}/one.bin"), 1);
    }
    Arc::new(fs)
}

fn shown_at(fs: &Arc<FakeFs>, path: &str, kind: VolumeKind, is_on: bool) -> Browser {
    let state = AppState::new(fs.location(path));
    let provider: Arc<dyn ProviderOps> = fs.clone();
    let mut browser = Browser::new(state, provider, Arc::new(|| {}));
    browser.set_volume_kinds(kinds(kind));
    browser.set_measures_shown_folders(is_on);
    browser
}

fn sizes(browser: &Browser) -> Vec<u64> {
    let tab = browser.state().active_tab().id();
    browser.details(tab).iter().map(|row| row.size).collect()
}

#[test]
fn a_listing_of_600_folders_measures_the_top_500() {
    let fs = many_folders(600);
    let mut browser = shown_at(&fs, "many", VolumeKind::Ssd, true);
    browser.start();
    settle(&mut browser);
    let sizes = sizes(&browser);
    assert_eq!(sizes.len(), 600);
    assert!(sizes[..500].iter().all(|&size| size == 1));
    assert!(sizes[500..].iter().all(|&size| size == 0));
    assert_eq!(fs.calls("many/f0500", FakeOp::List), 0);
}

#[test]
fn a_hard_disk_measures_its_shown_folders() {
    let fs = fake();
    let mut browser = shown_at(&fs, "home", VolumeKind::Hdd, true);
    browser.start();
    settle(&mut browser);
    assert_eq!(sizes(&browser)[0], 7);
}

#[test]
fn network_removable_and_unknown_volumes_measure_nothing() {
    for kind in [
        VolumeKind::Network,
        VolumeKind::Removable,
        VolumeKind::Unknown,
    ] {
        let fs = many_folders(3);
        let mut browser = shown_at(&fs, "many", kind, true);
        browser.start();
        settle(&mut browser);
        assert_eq!(sizes(&browser), [0, 0, 0], "{kind:?}");
        assert_eq!(fs.calls("many/f0000", FakeOp::List), 0, "{kind:?}");
    }
}

#[test]
fn with_the_setting_off_no_folder_is_measured() {
    let fs = many_folders(3);
    let mut browser = shown_at(&fs, "many", VolumeKind::Ssd, false);
    browser.start();
    settle(&mut browser);
    assert_eq!(sizes(&browser), [0, 0, 0]);
    assert_eq!(fs.calls("many/f0000", FakeOp::List), 0);
}

#[test]
fn a_space_still_measures_on_a_network_volume() {
    let fs = fake();
    let mut browser = shown_at(&fs, "home", VolumeKind::Network, true);
    browser.start();
    settle(&mut browser);
    browser.command(&Command::SelectionToggle).unwrap();
    settle(&mut browser);
    assert_eq!(sizes(&browser)[0], 7);
}

#[test]
fn a_list_sorted_by_size_keeps_its_rows_while_sizes_arrive() {
    let fs = FakeFs::new();
    fs.add_file("home/a/x.bin", 2);
    fs.add_file("home/b/x.bin", 3);
    fs.add_file("home/c/x.bin", 1);
    let fs = Arc::new(fs);
    let mut browser = shown_at(&fs, "home", VolumeKind::Ssd, true);
    let by_size = SortKey {
        by: SortBy::Size,
        descending: true,
    };
    browser.set_default_sort(by_size, true);
    browser.start();
    settle(&mut browser);
    assert_eq!(names(&browser), ["a", "b", "c"]);
    assert_eq!(sizes(&browser), [2, 3, 1]);
    browser.sort_by(SortBy::Size);
    assert_eq!(names(&browser), ["c", "a", "b"]);
}

#[test]
fn sizes_measured_before_a_refresh_still_show_after_it() {
    let fs = fake();
    let mut browser = shown_at(&fs, "home", VolumeKind::Ssd, true);
    browser.start();
    settle(&mut browser);
    fs.add_file("home/b.txt", 2);
    browser.command(&Command::PaneRefresh).unwrap();
    settle(&mut browser);
    assert_eq!(names(&browser), ["docs", "music", "a.txt", "b.txt"]);
    assert_eq!(sizes(&browser)[0], 7);
    assert_eq!(fs.calls("home/docs", FakeOp::List), 1);
}

fn gated_home() -> (Browser, Arc<FakeFs>, mpsc::Receiver<Walk>) {
    let fs = fake();
    fs.add_folder("away");
    let (walks, started) = mpsc::channel();
    let provider = Arc::new(Gated {
        fs: fs.clone(),
        open: std::sync::atomic::AtomicBool::new(false),
        walks: Some(walks),
    });
    let mut browser = Browser::new(
        AppState::new(fs.location("home")),
        provider,
        Arc::new(|| {}),
    );
    browser.set_volume_kinds(kinds(VolumeKind::Hdd));
    browser.set_measures_shown_folders(true);
    browser.start();
    (browser, fs, started)
}

#[test]
fn leaving_the_folder_cancels_the_shown_folder_sizes() {
    let (mut browser, fs, started) = gated_home();
    let deadline = Instant::now() + Duration::from_secs(5);
    let walk = loop {
        browser.poll();
        if let Ok(walk) = started.try_recv() {
            break walk;
        }
        assert!(Instant::now() < deadline, "the docs walk never started");
        std::thread::yield_now();
    };
    browser
        .command(&Command::PaneNavigate {
            location: fs.location("away"),
        })
        .unwrap();
    assert!(walk.finish(true));
    settle(&mut browser);
    assert_eq!(fs.calls("home/music", FakeOp::List), 0);
    assert!(!browser.loading());
}

fn gated_many() -> (Browser, Arc<FakeFs>, Walk) {
    let fs = FakeFs::new();
    fs.add_file("home/docs/x.bin", 7);
    for n in 0..600 {
        fs.add_file(&format!("home/f{n:04}/one.bin"), 1);
    }
    let fs = Arc::new(fs);
    let (walks, started) = mpsc::channel();
    let provider = Arc::new(Gated {
        fs: fs.clone(),
        open: std::sync::atomic::AtomicBool::new(false),
        walks: Some(walks),
    });
    let mut browser = Browser::new(
        AppState::new(fs.location("home")),
        provider,
        Arc::new(|| {}),
    );
    browser.set_volume_kinds(kinds(VolumeKind::Ssd));
    browser.set_measures_shown_folders(true);
    browser.start();
    let deadline = Instant::now() + Duration::from_secs(5);
    let walk = loop {
        browser.poll();
        if let Ok(walk) = started.try_recv() {
            break walk;
        }
        assert!(Instant::now() < deadline, "the docs walk never started");
        std::thread::yield_now();
    };
    (browser, fs, walk)
}

fn measured_order(browser: &Browser, count: usize) -> Vec<String> {
    let mut order = Vec::new();
    while order.len() < count {
        let message = browser
            .receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        if let Message::Measured(_, measured) = message {
            order.push(measured.name);
        }
    }
    order
}

#[test]
fn a_space_runs_before_the_queued_shown_folder_sizes() {
    let (mut browser, fs, walk) = gated_many();
    let tab = browser.state().active_tab().id();
    browser.measure(tab, "f0550".to_owned());
    assert!(!walk.finish(false));
    assert_eq!(measured_order(&browser, 2), ["docs", "f0550"]);
    assert_eq!(fs.calls("home/f0550", FakeOp::List), 1);
}

#[test]
fn a_space_on_a_queued_shown_folder_moves_that_job_ahead_without_a_second_walk() {
    let (mut browser, fs, walk) = gated_many();
    let tab = browser.state().active_tab().id();
    browser.measure(tab, "f0300".to_owned());
    assert!(!walk.finish(false));
    assert_eq!(measured_order(&browser, 2), ["docs", "f0300"]);
    assert_eq!(fs.calls("home/f0300", FakeOp::List), 1);
}

#[test]
fn turning_the_setting_off_cancels_the_queued_shown_folder_sizes() {
    let (mut browser, fs, walk) = gated_many();
    let tab = browser.state().active_tab().id();
    browser.measure(tab, "f0300".to_owned());
    browser.set_measures_shown_folders(false);
    assert!(walk.finish(true));
    settle(&mut browser);
    assert!(!browser.loading());
    assert_eq!(fs.calls("home/f0000", FakeOp::List), 0);
    assert_eq!(fs.calls("home/f0300", FakeOp::List), 1);
    assert_eq!(sizes(&browser)[301], 1);
}

#[test]
fn turning_the_setting_on_measures_the_shown_folders() {
    let fs = many_folders(3);
    let mut browser = shown_at(&fs, "many", VolumeKind::Ssd, false);
    browser.start();
    settle(&mut browser);
    browser.set_measures_shown_folders(true);
    settle(&mut browser);
    assert_eq!(sizes(&browser), [1, 1, 1]);
}
