use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::TestAppContext;
use rusty_fs::{FakeFs, FakeOp, ProfileState};

use super::{Rig, open_fake};
use crate::window_view::FerriteWindow;

const PATIENCE: Duration = Duration::from_secs(10);

fn watched() -> Arc<FakeFs> {
    let fs = FakeFs::new().with_watch();
    fs.add_file("home/proj/src/main.rs", 300);
    fs.add_file("home/proj/target/app.exe", 5_000);
    fs.add_file("home/proj/readme.md", 40);
    fs.add_file("home/zz.txt", 1);
    Arc::new(fs)
}

fn total(view: &FerriteWindow) -> Option<u64> {
    let profile = view.preview.folder()?.scan.profile.as_ref()?;
    (profile.state == ProfileState::Done).then_some(profile.totals.bytes)
}

fn label_until(cx: &mut TestAppContext, rig: &Rig, id: &'static str, want: &str) -> bool {
    let deadline = Instant::now() + PATIENCE;
    while Instant::now() < deadline {
        rig.rest(cx);
        if rig.label(cx, id).as_deref() == Some(want) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    false
}

fn is_clock(text: &str) -> bool {
    let Some(clock) = text.strip_prefix("Counted at ") else {
        return false;
    };
    let parts: Vec<&str> = clock.split(':').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| part.len() == 2 && part.bytes().all(|b| b.is_ascii_digit()))
}

fn opened_live(cx: &mut TestAppContext) -> (Rig, Arc<FakeFs>) {
    let fs = watched();
    let rig = open_fake(cx, &fs);
    rig.open(cx);
    assert!(rig.wait_until(cx, |view| total(view) == Some(5_340)));
    assert!(label_until(cx, &rig, "preview-live", "Live"));
    (rig, fs)
}

#[gpui_kit::test]
fn the_sheet_shows_live_and_the_count_time_and_ctrl_r_recounts(cx: &mut TestAppContext) {
    let (rig, fs) = opened_live(cx);
    let counted = rig.label(cx, "preview-counted").unwrap_or_default();
    assert!(is_clock(&counted), "{counted}");
    let listed = fs.calls("home/proj", FakeOp::List);
    rig.key(cx, "ctrl-shift-q");
    rig.key(cx, "ctrl-r");
    let deadline = Instant::now() + PATIENCE;
    while fs.calls("home/proj", FakeOp::List) == listed && Instant::now() < deadline {
        rig.rest(cx);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(fs.calls("home/proj", FakeOp::List), listed + 1);
    assert!(label_until(cx, &rig, "preview-live", "Live"));
}

#[gpui_kit::test]
fn a_file_written_deep_inside_updates_the_total(cx: &mut TestAppContext) {
    let (rig, fs) = opened_live(cx);
    let listed = fs.calls("home/proj", FakeOp::List);
    fs.add_file("home/proj/src/deep/er/new.rs", 1_000);
    assert!(rig.wait_until(cx, |view| total(view) == Some(6_340)));
    assert!(label_until(cx, &rig, "preview-total", "6,340"));
    assert_eq!(fs.calls("home/proj", FakeOp::List), listed);
}

#[gpui_kit::test]
fn closing_the_sheet_stops_the_watch(cx: &mut TestAppContext) {
    let (rig, fs) = opened_live(cx);
    assert_eq!(fs.watchers("home/proj"), 1);
    rig.key(cx, "escape");
    rig.settle_motion(cx);
    let deadline = Instant::now() + PATIENCE;
    while fs.watchers("home/proj") > 0 && Instant::now() < deadline {
        rig.rest(cx);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(fs.watchers("home/proj"), 0);
}

#[gpui_kit::test]
fn moving_to_another_row_stops_the_watch(cx: &mut TestAppContext) {
    let (rig, fs) = opened_live(cx);
    rig.key(cx, "down");
    rig.rest(cx);
    let deadline = Instant::now() + PATIENCE;
    while fs.watchers("home/proj") > 0 && Instant::now() < deadline {
        rig.rest(cx);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(fs.watchers("home/proj"), 0);
}
