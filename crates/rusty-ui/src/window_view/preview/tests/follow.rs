use gpui_kit::AppContext as _;
use std::sync::Arc;

use gpui_kit::TestAppContext;
use rusty_fs::FakeFs;

use super::gate::Gate;
use super::{Rig, open_fake, open_on};
use crate::window_view::FileHooks;
use crate::window_view::preview::Shown;

fn many_files() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    for index in 0..40 {
        fs.add_file(&format!("home/file{index:02}.txt"), 10);
    }
    Arc::new(fs)
}

fn sheet_left(cx: &mut TestAppContext, rig: &Rig) -> f32 {
    cx.update_window(rig.window, |_, window, cx| {
        use gpui_kit::test::TestWindowExt;
        window.render_frame(cx);
        f32::from(window.find("preview-sheet").bounds().origin.x)
    })
    .expect("window")
}

#[gpui_kit::test]
fn a_fast_arrow_burst_loads_once_where_the_cursor_stops(cx: &mut TestAppContext) {
    let rig = open_fake(cx, &many_files());
    rig.open(cx);
    assert_eq!(rig.loads(cx), 1);
    assert_eq!(rig.shown_name(cx).as_deref(), Some("file00.txt"));
    for _ in 0..12 {
        rig.key(cx, "down");
    }
    assert_eq!(rig.loads(cx), 1);
    rig.rest(cx);
    assert_eq!(rig.loads(cx), 2);
    assert_eq!(rig.shown_name(cx).as_deref(), Some("file12.txt"));
}

#[gpui_kit::test]
fn holding_an_arrow_through_many_rows_starts_only_a_few_loads(cx: &mut TestAppContext) {
    let rig = open_fake(cx, &many_files());
    rig.open(cx);
    for _ in 0..39 {
        rig.key(cx, "down");
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(33));
        cx.run_until_parked();
    }
    rig.rest(cx);
    assert!(rig.loads(cx) <= 3, "loads: {}", rig.loads(cx));
    assert_eq!(rig.shown_name(cx).as_deref(), Some("file39.txt"));
}

fn two_folders() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_file("home/first/inner/a.bin", 5);
    fs.add_file("home/second/b.bin", 7);
    Arc::new(fs)
}

#[gpui_kit::test]
fn a_newer_cursor_cancels_the_older_load_and_nothing_stale_lands(cx: &mut TestAppContext) {
    let fs = two_folders();
    let gate = Gate::new(&fs, "home/first/inner");
    let rig = open_on(cx, &fs, gate.clone(), FileHooks::none());
    rig.open(cx);
    assert_eq!(rig.shown_name(cx).as_deref(), Some("first"));
    assert!(rig.wait_until(cx, |_| gate.is_held()));
    rig.key(cx, "down");
    rig.rest(cx);
    gate.release();
    assert!(gate.wait_until_done(), "the older scan saw its cancel");
    assert!(rig.wait_until(cx, |view| {
        view.preview
            .folder()
            .is_some_and(|folder| !folder.scan.is_counting())
    }));
    assert_eq!(rig.shown_name(cx).as_deref(), Some("second"));
    let bytes = rig.read(cx, |view| {
        view.preview
            .folder()
            .and_then(|folder| folder.scan.profile.as_ref())
            .map(|profile| profile.totals.bytes)
    });
    assert_eq!(bytes, Some(7));
    assert_eq!(rig.read(cx, |view| view.browser.profiles().len()), 1);
}

#[gpui_kit::test]
fn tab_moves_the_sheet_to_the_other_side_and_follows_the_new_cursor(cx: &mut TestAppContext) {
    let fs = many_files();
    let rig = open_fake(cx, &fs);
    rig.open(cx);
    rig.key(cx, "down");
    rig.rest(cx);
    let before = sheet_left(cx, &rig);
    assert_eq!(rig.shown_name(cx).as_deref(), Some("file01.txt"));
    rig.key(cx, "tab");
    rig.rest(cx);
    let after = sheet_left(cx, &rig);
    assert!(
        (after - before).abs() > 100.,
        "sheet moved from {before} to {after}"
    );
    assert_eq!(rig.shown_name(cx).as_deref(), Some("file00.txt"));
    assert!(rig.list_has_focus(cx));
}

#[gpui_kit::test]
fn following_never_takes_focus_from_the_list(cx: &mut TestAppContext) {
    let rig = open_fake(cx, &many_files());
    rig.open(cx);
    for _ in 0..3 {
        rig.key(cx, "down");
        rig.rest(cx);
        assert!(rig.list_has_focus(cx));
    }
    let is_file = rig.read(cx, |view| {
        matches!(view.preview.shown(), Some(Shown::File(_)))
    });
    assert!(is_file);
}
