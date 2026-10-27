use std::sync::Arc;
use std::time::Duration;

use gpui_kit::TestAppContext;
use rusty_core::Location;
use rusty_fs::FakeFs;

use super::Rig;
use super::gate::Gate;
use crate::browser::{FolderTotals, FolderTotalsQuery};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{open_options, options_for, two_panes};

fn project() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_file("home/proj/src/main.rs", 300);
    fs.add_file("home/proj/target/app.exe", 5_000);
    fs.add_file("home/proj/readme.md", 240);
    fs.add_file("home/zz.txt", 1);
    Arc::new(fs)
}

fn index_answers(folder: Location, bytes: u64, complete: bool) -> FolderTotalsQuery {
    Arc::new(move |asked| {
        (asked == &folder).then_some(FolderTotals {
            bytes,
            files: 3,
            folders: 2,
            complete,
        })
    })
}

fn opened_gated(cx: &mut TestAppContext, bytes: u64, complete: bool) -> (Rig, Arc<Gate>) {
    cx.executor().allow_parking();
    let fs = project();
    let gate = Gate::new(&fs, "home/proj/target");
    let mut options = options_for(fs.clone(), two_panes(&fs));
    options.provider = gate.clone();
    options.search.folder_totals = index_answers(fs.location("home/proj"), bytes, complete);
    let (window, view) = open_options(cx, options);
    view.update(cx, |view, _| view.preview.set_every(Duration::ZERO));
    let rig = Rig { window, view };
    rig.open(cx);
    assert!(rig.wait_until(cx, |_| gate.is_held()));
    assert!(rig.wait_until(cx, |view| {
        view.preview
            .folder()
            .is_some_and(|folder| folder.index.is_some())
    }));
    (rig, gate)
}

fn size_column(view: &FerriteWindow) -> u64 {
    let tab = view.browser.state().active_tab().id();
    view.browser
        .details(tab)
        .iter()
        .find(|entry| entry.name.to_text() == "proj")
        .map_or(0, |entry| entry.size)
}

fn is_walked(view: &FerriteWindow) -> bool {
    view.preview
        .folder()
        .is_some_and(|folder| !folder.scan.is_counting())
}

#[gpui_kit::test]
fn complete_index_totals_show_at_once_in_the_preview_and_the_size_column(cx: &mut TestAppContext) {
    let (rig, gate) = opened_gated(cx, 9_000, true);
    assert_eq!(rig.label(cx, "preview-total").as_deref(), Some("9,000"));
    assert_eq!(rig.read(cx, size_column), 9_000);
    gate.release();
    assert!(rig.wait_until(cx, is_walked));
}

#[gpui_kit::test]
fn incomplete_index_totals_are_provisional_until_the_walk_ends(cx: &mut TestAppContext) {
    let (rig, gate) = opened_gated(cx, 2_000, false);
    assert_eq!(rig.label(cx, "preview-total").as_deref(), Some("2,000"));
    assert_eq!(rig.label(cx, "preview-scan").as_deref(), Some("Counting"));
    assert_ne!(rig.read(cx, size_column), 2_000);
    gate.release();
    assert!(rig.wait_until(cx, |view| is_walked(view) && size_column(view) == 5_540));
    assert_eq!(rig.label(cx, "preview-total").as_deref(), Some("5,540"));
}
