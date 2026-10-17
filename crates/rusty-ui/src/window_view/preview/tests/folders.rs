use gpui_kit::AppContext as _;
use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{ElementId, ScrollDelta, TestAppContext, point, px};
use rusty_fs::{FakeFs, FakeOp};

use super::gate::Gate;
use super::{Rig, open_fake, open_on};
use crate::window_view::FerriteWindow;
use crate::window_view::FileHooks;
use crate::window_view::preview::contents::{CHILD_LIMIT, RowKind};
use rusty_fs::{FolderProfile, ProfileState, Ranking, Tally};

fn project() -> FakeFs {
    let fs = FakeFs::new();
    fs.add_file("home/proj/src/main.rs", 300);
    fs.add_file("home/proj/src/lib.rs", 200);
    fs.add_file("home/proj/target/app.exe", 5_000);
    fs.add_file("home/proj/readme.md", 40);
    fs.add_file("home/zz.txt", 1);
    fs
}

fn done_profile() -> FolderProfile {
    FolderProfile {
        totals: Default::default(),
        newest_ms: None,
        kinds: Vec::new(),
        other: Tally::default(),
        largest: Vec::new(),
        ranking: Ranking::default(),
        counted_ms: None,
        state: ProfileState::Done,
    }
}

fn is_done(view: &FerriteWindow) -> bool {
    view.preview
        .folder()
        .is_some_and(|folder| folder.contents.is_read() && !folder.scan.is_counting())
}

fn total(view: &FerriteWindow) -> Option<u64> {
    let folder = view.preview.folder()?;
    folder
        .scan
        .profile
        .as_ref()
        .map(|profile| profile.totals.bytes)
}

fn top_of(cx: &mut TestAppContext, rig: &Rig, id: impl Into<ElementId>) -> f32 {
    let id = id.into();
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        f32::from(window.find(id).bounds().origin.y)
    })
    .expect("window")
}

fn opened(cx: &mut TestAppContext, fs: FakeFs) -> (Rig, Arc<FakeFs>) {
    let fs = Arc::new(fs);
    let rig = open_fake(cx, &fs);
    rig.open(cx);
    assert!(rig.wait_until(cx, is_done));
    (rig, fs)
}

#[gpui_kit::test]
fn a_folder_shows_its_contents_first_then_stats_types_and_the_largest_files(
    cx: &mut TestAppContext,
) {
    let (rig, _) = opened(cx, project());
    assert_eq!(
        rig.label(cx, ("preview-node", 0usize)).as_deref(),
        Some("src")
    );
    assert_eq!(
        rig.label(cx, ("preview-node", 2usize)).as_deref(),
        Some("readme.md")
    );
    let order = [
        top_of(cx, &rig, "preview-contents-heading"),
        top_of(cx, &rig, "preview-total"),
        top_of(cx, &rig, "preview-types-heading"),
        top_of(cx, &rig, "preview-largest-heading"),
    ];
    assert!(order.is_sorted(), "{order:?}");
    assert_eq!(rig.label(cx, "preview-total").as_deref(), Some("5,540"));
    assert_eq!(
        rig.label_any(cx, ("preview-largest", 0usize)).as_deref(),
        Some("target/app.exe")
    );
    assert_eq!(rig.label(cx, "preview-kind").as_deref(), Some("Folder"));
}

#[gpui_kit::test]
fn opening_a_node_reads_its_children_then(cx: &mut TestAppContext) {
    let (rig, fs) = opened(cx, project());
    assert_eq!(
        rig.read(cx, |view| view.preview.folder().map(|f| f.contents.len())),
        Some(3)
    );
    let walked = fs.calls("home/proj/src", FakeOp::List);
    rig.click(cx, ("preview-chevron", 0usize));
    assert!(rig.wait_until(cx, |view| {
        view.preview.folder().is_some_and(|f| f.contents.len() == 5)
    }));
    assert_eq!(fs.calls("home/proj/src", FakeOp::List), walked + 1);
    assert_eq!(
        rig.label(cx, ("preview-node", 1usize)).as_deref(),
        Some("lib.rs")
    );
    assert!(rig.list_has_focus(cx));
}

fn gated(cx: &mut TestAppContext) -> (Rig, Arc<Gate>) {
    let fs = Arc::new(project());
    let gate = Gate::new(&fs, "home/proj/target");
    let rig = open_on(cx, &fs, gate.clone(), FileHooks::none());
    rig.open(cx);
    assert!(rig.wait_until(cx, |_| gate.is_held()));
    assert!(rig.wait_until(cx, |view| total(view).is_some_and(|bytes| bytes > 0)));
    (rig, gate)
}

#[gpui_kit::test]
fn the_size_counter_grows_while_the_scan_runs(cx: &mut TestAppContext) {
    let (rig, gate) = gated(cx);
    let early = rig.read(cx, total).unwrap_or_default();
    assert!(early < 5_540, "{early}");
    assert_eq!(rig.label(cx, "preview-scan").as_deref(), Some("Counting"));
    gate.release();
    assert!(rig.wait_until(cx, is_done));
    assert_eq!(rig.read(cx, total), Some(5_540));
    let line = rig.label(cx, "preview-scan").unwrap_or_default();
    assert!(
        line.starts_with("Done in") && line.ends_with("cached"),
        "{line}"
    );
}

#[gpui_kit::test]
fn escape_stops_the_scan_and_resume_counts_again(cx: &mut TestAppContext) {
    let (rig, gate) = gated(cx);
    rig.key(cx, "escape");
    assert_eq!(
        rig.label(cx, "preview-scan").as_deref(),
        Some("Stopped, partial")
    );
    assert!(rig.shows(cx, "preview-resume"));
    assert!(rig.read(cx, |view| total(view).is_some_and(|bytes| bytes < 5_540)));
    gate.release();
    assert!(gate.wait_until_done());
    rig.click(cx, "preview-resume");
    assert!(rig.wait_until(cx, is_done));
    assert_eq!(rig.read(cx, total), Some(5_540));
    assert!(rig.list_has_focus(cx));
}

#[gpui_kit::test]
fn escape_clears_a_filter_then_stops_a_scan_then_closes_the_sheet(cx: &mut TestAppContext) {
    let (rig, gate) = gated(cx);
    rig.key(cx, "p");
    let filtered = rig.read(cx, |view| {
        let tab = view.browser.state().active_tab().id();
        view.browser.filter_text(tab).map(str::to_owned)
    });
    assert_eq!(filtered.as_deref(), Some("p"));
    rig.key(cx, "escape");
    let filtered = rig.read(cx, |view| {
        let tab = view.browser.state().active_tab().id();
        view.browser.filter_text(tab).map(str::to_owned)
    });
    assert_eq!(filtered, None);
    assert!(rig.read(cx, |view| {
        view.preview.folder().is_some_and(|f| f.scan.is_counting())
    }));
    rig.key(cx, "escape");
    assert_eq!(
        rig.label(cx, "preview-scan").as_deref(),
        Some("Stopped, partial")
    );
    gate.release();
    assert!(gate.wait_until_done());
    assert!(rig.read(cx, |view| view.preview.is_open()));
    rig.key(cx, "escape");
    assert!(!rig.read(cx, |view| view.preview.is_open()));
    assert!(rig.list_has_focus(cx));
}

#[gpui_kit::test]
fn a_cached_folder_answers_at_once_without_a_new_walk(cx: &mut TestAppContext) {
    let (rig, fs) = opened(cx, project());
    let walked = fs.calls("home/proj/target", FakeOp::List);
    rig.key(cx, "down");
    rig.rest(cx);
    rig.key(cx, "up");
    rig.rest(cx);
    assert_eq!(rig.shown_name(cx).as_deref(), Some("proj"));
    assert_eq!(rig.read(cx, total), Some(5_540));
    assert_eq!(
        rig.label(cx, "preview-scan").as_deref(),
        Some("Done · cached")
    );
    assert_eq!(fs.calls("home/proj/target", FakeOp::List), walked);
}

#[gpui_kit::test]
fn a_watch_event_drops_the_cached_profile(cx: &mut TestAppContext) {
    let (rig, fs) = opened(cx, project().with_watch());
    assert_eq!(rig.read(cx, |view| view.browser.profiles().len()), 1);
    fs.overflow_watchers("home");
    assert!(rig.wait_until(cx, |view| view.browser.profiles().is_empty()));
    let walked = fs.calls("home/proj/target", FakeOp::List);
    rig.key(cx, "down");
    rig.rest(cx);
    rig.key(cx, "up");
    rig.rest(cx);
    assert!(rig.wait_until(cx, is_done));
    assert_eq!(fs.calls("home/proj/target", FakeOp::List), walked + 1);
}

#[gpui_kit::test]
fn a_watch_change_drops_the_profiles_of_the_changed_folder_and_its_parents(
    cx: &mut TestAppContext,
) {
    let (rig, fs) = opened(cx, project().with_watch());
    rig.key(cx, "enter");
    rig.rest(cx);
    rig.key(cx, "down");
    rig.rest(cx);
    rig.key(cx, "enter");
    rig.rest(cx);
    let [proj, target, src] =
        ["home/proj", "home/proj/target", "home/proj/src"].map(|path| fs.location(path));
    rig.read(cx, |view| {
        for at in [&proj, &target, &src] {
            view.browser.profiles().put(at.clone(), 1, done_profile());
        }
    });
    fs.add_file("home/proj/target/new.txt", 1);
    assert!(rig.wait_until(cx, |view| view.browser.profiles().get(&target, 1).is_none()));
    rig.read(cx, |view| {
        assert!(view.browser.profiles().get(&proj, 1).is_none());
        assert!(view.browser.profiles().get(&src, 1).is_some());
    });
}

#[gpui_kit::test]
fn a_huge_folder_reaches_the_ui_already_cut_to_the_limit(cx: &mut TestAppContext) {
    let fs = FakeFs::new();
    for index in 0..CHILD_LIMIT + 500 {
        fs.add_file(&format!("home/big/f{index:05}"), 1);
    }
    let (rig, _) = opened(cx, fs);
    let rows = rig.read(cx, |view| {
        let contents = &view.preview.folder().expect("folder").contents;
        let last = contents.len() - 1;
        (
            contents.len(),
            contents
                .row(last)
                .map(|(kind, _)| matches!(kind, RowKind::More(500))),
        )
    });
    assert_eq!(rows, (CHILD_LIMIT + 1, Some(true)));
}

#[gpui_kit::test]
fn a_finished_scan_fills_the_size_column_of_its_row(cx: &mut TestAppContext) {
    let (rig, _) = opened(cx, project());
    let size = rig.read(cx, |view| {
        let tab = view.browser.state().active_tab().id();
        view.browser.details(tab).first().map(|entry| entry.size)
    });
    assert_eq!(size, Some(5_540));
}

fn many_kinds() -> FakeFs {
    let fs = FakeFs::new();
    for index in 0..40 {
        fs.add_file(&format!("home/mix/file.k{index:02}"), 100 + index);
    }
    fs
}

#[gpui_kit::test]
fn the_section_heading_stays_pinned_and_opaque_while_the_summary_scrolls(cx: &mut TestAppContext) {
    let (rig, _) = opened(cx, many_kinds());
    assert!(!rig.shows(cx, "preview-pinned-heading"));
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.scroll(
            "preview-summary",
            ScrollDelta::Pixels(point(px(0.), px(-400.))),
            cx,
        );
    })
    .expect("window");
    rig.redraw(cx);
    assert_eq!(
        rig.label(cx, "preview-pinned-heading").as_deref(),
        Some("By type")
    );
}
