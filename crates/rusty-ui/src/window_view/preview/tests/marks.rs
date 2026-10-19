use std::sync::Arc;

use gpui_kit::TestAppContext;
use rusty_fs::FakeFs;

use super::gate::Gate;
use super::{Rig, open_fake, open_on};
use crate::format;
use crate::window_view::FerriteWindow;
use crate::window_view::FileHooks;
use crate::window_view::preview::contents::RowKind;
use crate::window_view::preview::spot::Spot;

fn project() -> FakeFs {
    let fs = FakeFs::new();
    fs.add_file("home/proj/src/main.rs", 300);
    fs.add_file("home/proj/src/lib.rs", 200);
    fs.add_file("home/proj/target/app.exe", 5_000);
    fs.add_file("home/proj/readme.md", 40);
    fs.add_file("home/zz.txt", 1);
    fs
}

fn is_done(view: &FerriteWindow) -> bool {
    view.preview
        .folder()
        .is_some_and(|folder| folder.contents.is_read() && !folder.scan.is_counting())
}

fn browsing(cx: &mut TestAppContext) -> (Rig, Arc<FakeFs>) {
    let fs = Arc::new(project());
    let rig = open_fake(cx, &fs);
    rig.open(cx);
    assert!(rig.wait_until(cx, is_done));
    rig.key(cx, "ctrl-shift-q");
    (rig, fs)
}

fn spot(cx: &mut TestAppContext, rig: &Rig) -> Option<String> {
    rig.read(cx, |view| {
        let folder = view.preview.folder()?;
        match folder.spot? {
            Spot::Largest(rank) => Some(format!("largest {rank}")),
            Spot::Tree(_) => match folder.contents.row(folder.spot_row()?)?.0 {
                RowKind::Entry(entry) => Some(entry.name.to_text()),
                RowKind::More(_) => None,
            },
        }
    })
}

fn marked(view: &FerriteWindow) -> (usize, Option<u64>) {
    view.preview.folder().map_or((0, None), |folder| {
        (folder.marks.len(), folder.marks.total())
    })
}

fn status(cx: &mut TestAppContext, rig: &Rig) -> Option<String> {
    rig.label(cx, "side-sheet-status")
}

#[gpui_kit::test]
fn space_marks_the_folder_counts_its_size_and_moves_down(cx: &mut TestAppContext) {
    let (rig, fs) = browsing(cx);
    rig.key(cx, "space");
    assert_eq!(spot(cx, &rig).as_deref(), Some("target"));
    assert!(rig.wait_until(cx, |view| marked(view) == (1, Some(500))));
    let src = fs.location("home/proj/src");
    assert!(rig.read(cx, |view| {
        view.preview.folder().is_some_and(|folder| {
            folder.marks.is_marked(&src) && folder.marks.size(&src) == Some(500)
        })
    }));
    assert_eq!(
        rig.label(cx, ("preview-node-size", 0usize)).as_deref(),
        Some(format::size(500).as_str())
    );
}

#[gpui_kit::test]
fn the_footer_shows_the_marked_count_and_total_size(cx: &mut TestAppContext) {
    let (rig, _) = browsing(cx);
    assert_eq!(status(cx, &rig), None);
    for _ in 0..3 {
        rig.key(cx, "space");
    }
    assert!(rig.wait_until(cx, |view| marked(view) == (3, Some(5_540))));
    assert_eq!(
        status(cx, &rig),
        Some(format!("3 marked · {}", format::size(5_540)))
    );
    let hints: Vec<String> = (0..5usize)
        .filter_map(|index| rig.label(cx, ("side-sheet-hint", index)))
        .collect();
    assert_eq!(hints, ["Open", "Go to", "Mark", "Recount", "Close"]);
}

#[gpui_kit::test]
fn space_on_a_marked_row_unmarks_it(cx: &mut TestAppContext) {
    let (rig, _) = browsing(cx);
    rig.key(cx, "down");
    rig.key(cx, "down");
    rig.key(cx, "space");
    assert!(rig.wait_until(cx, |view| marked(view) == (1, Some(40))));
    rig.key(cx, "up");
    assert_eq!(spot(cx, &rig).as_deref(), Some("readme.md"));
    rig.key(cx, "space");
    assert_eq!(rig.read(cx, marked), (0, Some(0)));
    assert_eq!(status(cx, &rig), None);
}

#[gpui_kit::test]
fn marks_clear_when_the_root_changes(cx: &mut TestAppContext) {
    let (rig, _) = browsing(cx);
    rig.key(cx, "space");
    assert!(rig.wait_until(cx, |view| marked(view).0 == 1));
    rig.key(cx, "enter");
    assert!(rig.wait_until(cx, |view| {
        is_done(view)
            && view
                .preview
                .folder()
                .is_some_and(|f| f.target.name() == "target")
    }));
    assert_eq!(rig.read(cx, marked).0, 0);
    rig.key(cx, "backspace");
    assert!(rig.wait_until(cx, |view| {
        is_done(view)
            && view
                .preview
                .folder()
                .is_some_and(|f| f.target.name() == "proj")
    }));
    assert_eq!(rig.read(cx, marked).0, 0);
    assert_eq!(status(cx, &rig), None);
}

#[gpui_kit::test]
fn marks_clear_when_the_preview_closes(cx: &mut TestAppContext) {
    let (rig, _) = browsing(cx);
    rig.key(cx, "space");
    assert!(rig.wait_until(cx, |view| marked(view).0 == 1));
    rig.key(cx, "escape");
    assert_eq!(rig.read(cx, marked).0, 0);
    rig.open(cx);
    assert!(rig.wait_until(cx, is_done));
    assert_eq!(rig.read(cx, marked).0, 0);
    assert_eq!(status(cx, &rig), None);
}

#[gpui_kit::test]
fn unmarking_a_folder_stops_its_running_count(cx: &mut TestAppContext) {
    let fs = Arc::new(project());
    let gate = Gate::new(&fs, "home/proj/target");
    let rig = open_on(cx, &fs, gate.clone(), FileHooks::none());
    rig.open(cx);
    gate.release();
    assert!(rig.wait_until(cx, is_done));
    rig.key(cx, "ctrl-shift-q");
    rig.key(cx, "down");
    gate.arm();
    rig.key(cx, "space");
    assert!(rig.wait_until(cx, |_| gate.is_held()));
    let target = fs.location("home/proj/target");
    assert!(rig.read(cx, |view| {
        view.preview
            .folder()
            .is_some_and(|folder| folder.marks.is_counting(&target))
    }));
    rig.key(cx, "up");
    rig.key(cx, "space");
    assert!(rig.read(cx, |view| {
        view.preview
            .folder()
            .is_some_and(|folder| !folder.marks.is_counting(&target))
    }));
    gate.release();
    assert!(gate.wait_until_done());
    assert_eq!(rig.read(cx, marked), (0, Some(0)));
}

#[gpui_kit::test]
fn marks_survive_a_file_opened_in_the_preview(cx: &mut TestAppContext) {
    let (rig, _) = browsing(cx);
    rig.key(cx, "space");
    assert!(rig.wait_until(cx, |view| marked(view) == (1, Some(500))));
    rig.key(cx, "down");
    assert_eq!(spot(cx, &rig).as_deref(), Some("readme.md"));
    rig.key(cx, "enter");
    assert!(rig.wait_until(cx, |view| view.preview.folder().is_none()));
    rig.key(cx, "backspace");
    assert!(rig.wait_until(cx, is_done));
    assert_eq!(rig.read(cx, marked), (1, Some(500)));
    assert_eq!(
        status(cx, &rig),
        Some(format!("1 marked · {}", format::size(500)))
    );
}
