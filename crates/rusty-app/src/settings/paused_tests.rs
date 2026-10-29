use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use rusty_ui::window_view::SettingsNews;

use super::{Loaded, hooks, watched};

const WAIT: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_secs(2);
const BROKEN: &str = "# mine\n[retention]\nretain-deletes = \n";
const FIXED: &str = "# mine\n[retention]\nretain-deletes = false\n";
const BAD_FONT: &str = "[appearance]\nfont-size = \"big\"\n";

fn file(text: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("navo.toml");
    fs::write(&path, text).expect("write");
    (dir, path)
}

fn notice(path: &Path, guards: bool) -> Option<String> {
    let loaded = Loaded::load(path).guarding_deletes(guards);
    hooks(Some(loaded)).load_error.map(|text| text.to_string())
}

#[test]
fn a_broken_file_says_deletes_are_paused_only_when_jobs_guard_them() {
    let (_dir, path) = file(BROKEN);
    let plain = notice(&path, false).expect("a notice");
    let paused = notice(&path, true).expect("a notice");
    assert!(plain.contains("line 3"), "{plain}");
    assert!(paused.contains(&plain), "{paused}");
    assert_ne!(paused, plain);
}

#[test]
fn an_error_outside_retention_does_not_claim_deletes_are_paused() {
    let (_dir, path) = file(BAD_FONT);
    let plain = notice(&path, false).expect("a notice");
    assert_eq!(notice(&path, true), Some(plain));
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn the_paused_notice_shows_once_and_a_fix_clears_it_live() {
    let (_dir, path) = file(BROKEN);
    let (sender, news) = mpsc::channel();
    let loaded = Loaded::load(&path).guarding_deletes(true);
    let (hooks, watching) = watched(Some(loaded), move |news| {
        let _ = sender.send(news);
    });
    let watching = watching.expect("the settings folder is watched");
    let deadline = Instant::now() + WAIT;
    while !watching.is_live() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let start = hooks.load_error.expect("a start-up notice").to_string();
    assert!(news.recv_timeout(QUIET).is_err(), "the notice repeated");
    fs::write(&path, FIXED).expect("write");
    assert!(matches!(
        news.recv_timeout(WAIT).expect("news"),
        SettingsNews::Applied { .. }
    ));
    fs::write(&path, BROKEN).expect("write");
    match news.recv_timeout(WAIT).expect("news") {
        SettingsNews::Invalid(message) => assert_eq!(message, start),
        other => panic!("expected the paused notice, got {other:?}"),
    }
}

#[test]
fn the_profile_start_carries_the_guard_to_the_notice() {
    let root = tempfile::tempdir().expect("dir");
    let dirs = rusty_store::StoreDirs::in_profile(root.path());
    dirs.ensure().expect("dirs");
    fs::write(dirs.settings_file(), BROKEN).expect("write");
    let started = |guards| {
        let loaded = super::start_from((Some(&dirs), None), guards)
            .finish()
            .into_loaded();
        hooks(loaded).load_error.map(|text| text.to_string())
    };
    let plain = started(false).expect("a notice");
    let paused = started(true).expect("a notice");
    assert!(paused.contains(&plain), "{paused}");
    assert_ne!(paused, plain);
}
