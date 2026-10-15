use std::rc::Rc;

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Role, TestAppContext};
use rusty_core::{Selection, ViewState};

use super::{MODIFIED_MS, generated_rows, open_list, props, sample_rows};
use crate::file_list::{kind, row_semantics};
use crate::format;
use rusty_fs::EntryKind;

const ROWS: usize = 100_000;

#[gpui_kit::test]
fn a_hundred_thousand_rows_render_only_the_visible_ones(cx: &mut TestAppContext) {
    let (window, _) = open_list(cx, props(generated_rows(ROWS)));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let list = window.within("left").find("list");
        assert_eq!(list.role(), Some(Role::List));
        assert_eq!(list.label(), Some("core files"));
        let left = window.within("left");
        assert!(left.find(("row", 0usize)).visible());
        assert!(left.find(("row", 18usize)).visible());
        assert!(left.try_find(("row", 40usize)).is_none());
        assert!(left.try_find(("row", ROWS - 1)).is_none());
    })
    .expect("window");
}

#[gpui_kit::test]
fn the_scroll_row_comes_to_the_top(cx: &mut TestAppContext) {
    let mut scrolled = props(generated_rows(ROWS));
    scrolled.view = ViewState {
        cursor: 50_000,
        scroll: 50_000,
    };
    let (window, _) = open_list(cx, scrolled);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let left = window.within("left");
        assert!(left.find(("row", 50_000usize)).visible());
        assert!(left.try_find(("row", 0usize)).is_none());
    })
    .expect("window");
}

#[gpui_kit::test]
fn rows_carry_their_name_and_marked_state(cx: &mut TestAppContext) {
    let mut marked = props(sample_rows());
    let mut selection = Selection::new(6);
    selection.toggle(1);
    selection.toggle(3);
    marked.marked = Rc::new(selection);
    let (window, _) = open_list(cx, marked);
    let date = format::date(MODIFIED_MS);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let left = window.within("left");
        let rows = [
            (0usize, "src, folder".to_owned(), false),
            (1, "tests, folder".to_owned(), true),
            (2, format!("Cargo.toml, 812 bytes, {date}"), false),
            (3, format!("layout.rs, 14,336 bytes, {date}"), true),
        ];
        for (index, label, is_marked) in rows {
            let row = left.find(("row", index));
            assert_eq!(row.role(), Some(Role::ListItem), "row {index}");
            assert_eq!(row.label(), Some(label.as_str()), "row {index}");
            assert_eq!(row.selected(), Some(is_marked), "row {index}");
        }
    })
    .expect("window");
}

#[test]
fn the_cursor_row_is_the_active_descendant_of_the_active_pane() {
    let mut active = props(sample_rows());
    active.view = ViewState {
        cursor: 2,
        scroll: 0,
    };
    assert!(row_semantics(&active, 2).is_active_descendant);
    assert!(!row_semantics(&active, 1).is_active_descendant);
    let idle = super::super::FileListProps {
        is_active: false,
        ..active
    };
    assert!(!row_semantics(&idle, 2).is_active_descendant);
}

#[test]
fn names_split_into_name_and_extension_like_total_commander() {
    assert_eq!(
        kind::split_name("Cargo.toml", EntryKind::File),
        ("Cargo", "toml")
    );
    assert_eq!(
        kind::split_name("archive.tar.gz", EntryKind::File),
        ("archive.tar", "gz")
    );
    assert_eq!(kind::split_name(".env", EntryKind::File), (".env", ""));
    assert_eq!(kind::split_name("README", EntryKind::File), ("README", ""));
    assert_eq!(kind::split_name("v1.2", EntryKind::Folder), ("v1.2", ""));
}

#[gpui_kit::test]
fn the_scroll_row_is_applied_once_the_rows_arrive(cx: &mut TestAppContext) {
    let view = ViewState {
        cursor: 500,
        scroll: 500,
    };
    let mut empty = props(generated_rows(0));
    empty.view = view;
    let (window, harness) = open_list(cx, empty);
    harness.update(cx, |harness, cx| {
        let mut loaded = props(generated_rows(1_000));
        loaded.view = view;
        harness.props = loaded;
        cx.notify();
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let left = window.within("left");
        assert!(left.find(("row", 500usize)).visible());
        assert!(left.try_find(("row", 0usize)).is_none());
    })
    .expect("window");
}

#[test]
fn a_measured_folder_shows_its_size_instead_of_dir() {
    let mut folder = super::entry("src", EntryKind::Folder, 0);
    assert_eq!(kind::size_text(&folder), "<DIR>");
    folder.size = 1_234_567;
    assert_eq!(kind::size_text(&folder), "1.2 MB");
}
