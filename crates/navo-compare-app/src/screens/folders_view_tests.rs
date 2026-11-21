//! Tests of `FoldersView`: cursor keys, Enter on a file, and the summary chips.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, Focusable as _, TestAppContext, px, size,
};
use navo_compare_profile::appearance::PaletteId;
use sidewise_kit::tokens::Mode;
use sidewise_ui::browser::{CompareKind, CompareRow, CompareSide, RowStatus};

use sidewise_ui::compare_diff::map::MapEvent;

use super::{FoldersEvent, FoldersView, bind_keys};
use crate::screens::compare_tab::{CompareRequest, title_of};
use crate::screens::folders_head::SideRoot;
use crate::test_window::open;
use crate::theme::apply_sidewise;
use navo_compare_cli::args::SideSpec;

/// A side of `kind` with a placeholder size.
fn side(kind: CompareKind) -> Option<CompareSide> {
    Some(CompareSide {
        kind,
        size: 1,
        modified_ms: None,
    })
}

/// A row present on both sides.
fn row(id: u64, path: &str, status: RowStatus, kind: CompareKind) -> CompareRow {
    CompareRow::new(id, path, status, (side(kind), side(kind)))
}

/// The U17a' fixture: folders `a`, `a/b`, `c`, with files under them and `z.txt`.
fn rows() -> Vec<CompareRow> {
    use CompareKind::{File, Folder};
    let only_left = CompareRow::new(4, "a/b/y.txt", RowStatus::OnlyLeft, (side(File), None));
    vec![
        row(1, "a", RowStatus::Different, Folder),
        row(2, "a/x.txt", RowStatus::Different, File),
        row(3, "a/b", RowStatus::Different, Folder),
        only_left,
        row(5, "c", RowStatus::Same, Folder),
        row(6, "c/s.txt", RowStatus::Same, File),
        row(7, "z.txt", RowStatus::Same, File),
    ]
}

/// A focused view over the fixture, with the events it emits collected.
pub(super) fn view(
    cx: &mut TestAppContext,
) -> (
    AnyWindowHandle,
    Entity<FoldersView>,
    Rc<RefCell<Vec<FoldersEvent>>>,
) {
    view_with(cx, FoldersView::new)
}

/// Like `view`, with `make` building the view (for example with roots).
pub(super) fn view_with(
    cx: &mut TestAppContext,
    make: impl FnOnce(&mut gpui_kit::Context<FoldersView>) -> FoldersView + 'static,
) -> (
    AnyWindowHandle,
    Entity<FoldersView>,
    Rc<RefCell<Vec<FoldersEvent>>>,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        apply_sidewise(PaletteId::Violet, Mode::Light, cx);
        bind_keys(cx);
    });
    let (window, view) = open(cx, |_, cx| make(cx));
    let events = Rc::new(RefCell::new(Vec::new()));
    let seen = events.clone();
    cx.update(|cx| {
        cx.subscribe(&view, move |_, event: &FoldersEvent, _| {
            seen.borrow_mut().push(*event)
        })
        .detach();
    });
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |this, cx| this.set_rows(rows(), cx));
        view.read(cx).focus_handle(cx).focus(window, cx);
        window.render_frame(cx);
    })
    .expect("window");
    (window, view, events)
}

/// The paths of the lines shown now.
pub(super) fn shown(view: &Entity<FoldersView>, cx: &mut TestAppContext) -> Vec<String> {
    cx.update(|cx| {
        view.read(cx)
            .tree
            .lines()
            .into_iter()
            .map(|line| line.path)
            .collect()
    })
}

/// Right on a closed folder opens it; right again moves to its first child; left goes back up.
#[gpui_kit::test]
fn right_opens_a_folder_then_enters_it(cx: &mut TestAppContext) {
    let (window, view, _) = view(cx);
    assert_eq!(shown(&view, cx), ["a", "c", "z.txt"]);
    cx.simulate_keystrokes(window, "right");
    assert_eq!(shown(&view, cx), ["a", "a/x.txt", "a/b", "c", "z.txt"]);
    cx.simulate_keystrokes(window, "right");
    cx.update(|cx| assert_eq!(view.read(cx).selected_path().as_deref(), Some("a/x.txt")));
    cx.simulate_keystrokes(window, "left");
    cx.update(|cx| assert_eq!(view.read(cx).selected_path().as_deref(), Some("a")));
    cx.simulate_keystrokes(window, "left");
    assert_eq!(shown(&view, cx), ["a", "c", "z.txt"]);
}

/// Enter on `a/x.txt` emits `OpenPair(2)`; Enter on a folder emits nothing.
#[gpui_kit::test]
fn enter_on_a_file_opens_its_pair(cx: &mut TestAppContext) {
    let (window, view, events) = view(cx);
    cx.simulate_keystrokes(window, "enter");
    assert!(events.borrow().is_empty(), "a folder only toggles");
    assert_eq!(shown(&view, cx).len(), 5);
    cx.simulate_keystrokes(window, "down");
    cx.update(|cx| assert_eq!(view.read(cx).selected_path().as_deref(), Some("a/x.txt")));
    cx.simulate_keystrokes(window, "enter");
    assert_eq!(*events.borrow(), [FoldersEvent::OpenPair(2)]);
}

/// With `a` open, a click on the Differences chip leaves `a`, `a/x.txt` and `a/b`.
#[gpui_kit::test]
fn differences_chip_filters_the_tree(cx: &mut TestAppContext) {
    let (window, view, _) = view(cx);
    cx.simulate_keystrokes(window, "right");
    cx.update_window(window, |_, window, cx| window.click("chip-Differences", cx))
        .expect("window");
    assert_eq!(shown(&view, cx), ["a", "a/x.txt", "a/b"]);
}

/// `select_path` opens the ancestors and puts the cursor on the file.
#[gpui_kit::test]
fn select_path_opens_ancestors(cx: &mut TestAppContext) {
    let (_, view, _) = view(cx);
    let found = cx.update(|cx| view.update(cx, |this, cx| this.select_path("a/b/y.txt", cx)));
    assert!(found);
    assert_eq!(
        shown(&view, cx),
        ["a", "a/x.txt", "a/b", "a/b/y.txt", "c", "z.txt"]
    );
    cx.update(|cx| assert_eq!(view.read(cx).selected_path().as_deref(), Some("a/b/y.txt")));
    assert!(!cx.update(|cx| view.update(cx, |this, cx| this.select_path("nope", cx))));
}

/// With `a` open, a `Jump(2)` from the change map puts the cursor on `a/b`.
#[gpui_kit::test]
fn map_jump_moves_the_cursor(cx: &mut TestAppContext) {
    let (window, view, _) = view(cx);
    cx.simulate_keystrokes(window, "right");
    let map = cx.update(|cx| view.read(cx).map.clone());
    cx.update(|cx| map.update(cx, |_, cx| cx.emit(MapEvent::Jump(2))));
    cx.update(|cx| assert_eq!(view.read(cx).selected_path().as_deref(), Some("a/b")));
}

/// The label of the element `id` after a render.
fn label_of(
    window: AnyWindowHandle,
    id: (&'static str, usize),
    cx: &mut TestAppContext,
) -> Option<String> {
    cx.update_window(window, |_, window, _| {
        window.find(id).label().map(String::from)
    })
    .expect("window")
}

/// Line 2 (`z.txt`) shows its name and size on the left; line 0 (`a`) shows its items and the
/// `2 inside` gutter.
#[gpui_kit::test]
fn a_row_shows_both_cells_and_its_gutter(cx: &mut TestAppContext) {
    let (window, _, _) = view(cx);
    assert_eq!(
        label_of(window, ("cell-l", 2), cx).as_deref(),
        Some("z.txt · 1 B")
    );
    assert_eq!(label_of(window, ("gut", 2), cx), None);
    assert_eq!(
        label_of(window, ("cell-l", 0), cx).as_deref(),
        Some("a · 2 items")
    );
    assert_eq!(
        label_of(window, ("gut", 0), cx).as_deref(),
        Some("2 inside")
    );
}

/// A file only on the left reads `not in right` on the right and `Only left` in the gutter.
#[gpui_kit::test]
fn a_missing_side_reads_not_in(cx: &mut TestAppContext) {
    let (window, view, _) = view(cx);
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |this, cx| this.select_path("a/b/y.txt", cx));
        window.render_frame(cx);
    })
    .expect("window");
    assert_eq!(
        label_of(window, ("cell-r", 3), cx).as_deref(),
        Some("not in right")
    );
    assert_eq!(
        label_of(window, ("gut", 3), cx).as_deref(),
        Some("Only left")
    );
}

/// The orphan chip is named `Orphans` and counts the one file only on the left.
#[gpui_kit::test]
fn the_chips_say_orphans(cx: &mut TestAppContext) {
    let (window, _, _) = view(cx);
    cx.update_window(window, |_, window, _| {
        assert_eq!(window.find("chip-Orphans").label(), Some("Orphans 1"));
    })
    .expect("window");
}

/// Each side's header names its folder root: the label is the full path.
#[gpui_kit::test]
fn the_headers_name_both_roots(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        apply_sidewise(PaletteId::Violet, Mode::Light, cx);
    });
    let roots = [
        SideRoot::folder(abs_root("work").join("left")),
        SideRoot::folder(abs_root("work").join("right")),
    ];
    let (window, _) = open(cx, |_, cx| FoldersView::new(cx).with_roots(roots));
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    assert_eq!(
        label_of(window, ("head", 0), cx).as_deref(),
        abs_root("work").join("left").to_str()
    );
    assert_eq!(
        label_of(window, ("head", 1), cx).as_deref(),
        abs_root("work").join("right").to_str()
    );
}

/// A Git side's header reads `path @ rev`; a folder side keeps the bare path.
#[gpui_kit::test]
fn a_git_root_draws_its_revision(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        apply_sidewise(PaletteId::Violet, Mode::Light, cx);
    });
    let repo = abs_root("work").join("app");
    let roots = [
        SideRoot {
            path: repo.clone(),
            rev: Some("main".to_owned()),
        },
        SideRoot {
            path: abs_root("work").join("right"),
            rev: None,
        },
    ];
    let (window, _) = open(cx, |_, cx| FoldersView::new(cx).with_roots(roots));
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    let want = format!("{} @ main", repo.display());
    assert_eq!(
        label_of(window, ("head", 0), cx).as_deref(),
        Some(want.as_str())
    );
    assert_eq!(
        label_of(window, ("head", 1), cx).as_deref(),
        abs_root("work").join("right").to_str()
    );
}

/// A remote side's header reads `host/path @ rev`, never joined to the working folder.
#[gpui_kit::test]
fn a_remote_root_is_not_joined_to_the_working_folder(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        apply_sidewise(PaletteId::Violet, Mode::Light, cx);
    });
    let roots = [
        SideRoot {
            path: PathBuf::from("github.com/owner/repo"),
            rev: Some("main".to_owned()),
        },
        SideRoot::folder(abs_root("work").join("right")),
    ];
    let (window, _) = open(cx, |_, cx| FoldersView::new(cx).with_roots(roots));
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    let want = format!("{} @ main", Path::new("github.com/owner/repo").display());
    assert_eq!(
        label_of(window, ("head", 0), cx).as_deref(),
        Some(want.as_str())
    );
}

/// A Git side and a folder title the tab `app@main ↔ app`.
#[test]
fn a_git_and_folder_title_reads_name_at_rev() {
    let request = CompareRequest {
        left: SideSpec::GitRev {
            repo: PathBuf::from("/w/app"),
            rev: "main".to_owned(),
        },
        right: SideSpec::Path(PathBuf::from("/w/app")),
        choice: Default::default(),
    };
    assert_eq!(title_of(&request).as_ref(), "app@main ↔ app");
}

/// The meta text and the gutter width follow the view width: wide shows size, narrow names only.
#[gpui_kit::test]
fn meta_follows_the_view_width(cx: &mut TestAppContext) {
    let (window, _, _) = view(cx);
    let gut_w = |cx: &mut TestAppContext| {
        cx.update_window(window, |_, window, _| {
            f32::from(window.find(("gut", 2usize)).bounds().size.width)
        })
        .expect("window")
    };
    for (w, text, gut) in [(1000., "z.txt · 1 B", 98.), (500., "z.txt", 40.)] {
        cx.simulate_window_resize(window, size(px(w), px(600.)));
        cx.update_window(window, |_, window, cx| window.render_frame(cx))
            .expect("window");
        cx.update_window(window, |_, window, cx| window.render_frame(cx))
            .expect("window");
        assert_eq!(label_of(window, ("cell-l", 2), cx).as_deref(), Some(text));
        assert_eq!(gut_w(cx), gut);
    }
}

/// An absolute test root: `/name` on Unix, `C:\name` on Windows, where a path without a drive
/// is not absolute and the app shows it drive-qualified.
fn abs_root(name: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(format!("C:\\{name}"))
    } else {
        PathBuf::from(format!("/{name}"))
    }
}

/// B7: the meta mode measured during a frame must notify the view after the frame, or the
/// window keeps the stale columns until the next input event.
#[gpui_kit::test]
fn meta_change_in_a_frame_asks_for_a_redraw(cx: &mut TestAppContext) {
    use crate::chrome::width_redraw_tests::{count_notifies, one_frame_at};
    let (window, view, _) = view(cx);
    let notes = count_notifies(&view, cx);
    one_frame_at(window, 500., cx);
    assert!(notes.get() > 0, "the narrow meta mode was never redrawn");
}
