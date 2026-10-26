use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, InputEvent as _, MouseButton, MouseDownEvent,
    MouseUpEvent, Pixels, Point, Role, TestAppContext, point, px,
};

use super::structured::open_named;
use super::{Setup, editor_of, redraw, text_window};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{press, viewer_open};

pub(super) const LINK: &str = "a link whose words run on and on so that it fills every line of the \
    paragraph from the left edge to the right edge and keeps going for a while longer, \
    long enough to wrap several times across the reading column in any window size";

pub(super) fn click_with(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    (button, at): (MouseButton, Point<Pixels>),
    down: bool,
) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let event = if down {
            MouseDownEvent {
                button,
                position: at,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input()
        } else {
            MouseUpEvent {
                button,
                position: at,
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input()
        };
        window.dispatch_event(event, cx);
    })
    .expect("window");
    cx.run_until_parked();
}

pub(super) fn right_click(cx: &mut TestAppContext, window: AnyWindowHandle, at: Point<Pixels>) {
    click_with(cx, window, (MouseButton::Right, at), true);
    click_with(cx, window, (MouseButton::Right, at), false);
    redraw(cx, window);
}

pub(super) fn middle(cx: &mut TestAppContext, window: AnyWindowHandle) -> Point<Pixels> {
    cx.update_window(window, |_, window, _| {
        let size = window.viewport_size();
        point(size.width / 2., size.height / 2.)
    })
    .expect("window")
}

pub(super) fn tree_row_at(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    row: usize,
) -> Point<Pixels> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.find(("tree-row", row)).bounds().center()
    })
    .expect("window")
}

pub(super) fn table_cell_at(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    (row, column): (usize, usize),
) -> Point<Pixels> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let line = window.within(("table-row", row));
        line.find(("table-cell", column)).bounds().center()
    })
    .expect("window")
}

pub(super) fn table_head_at(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    column: usize,
) -> Point<Pixels> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.find(("table-head", column)).bounds().center()
    })
    .expect("window")
}

pub(super) fn rows(cx: &mut TestAppContext, window: AnyWindowHandle) -> Vec<String> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        if window.try_find("context-menu").is_none() {
            return Vec::new();
        }
        let mut root = window.within("context-menu");
        let level = root.within(("menu-level", 0usize));
        (0usize..)
            .map_while(|index| level.try_find(("menu-row", index)))
            .map(|row| match (row.role(), row.label()) {
                (Some(Role::Heading), Some(label)) => format!("# {label}"),
                (_, Some(label)) => label.to_owned(),
                (_, None) => "-".to_owned(),
            })
            .collect()
    })
    .expect("window")
}

fn menu_bounds(cx: &mut TestAppContext, window: AnyWindowHandle) -> Bounds<Pixels> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let root = window.within("context-menu");
        root.find(("menu-level", 0usize)).bounds()
    })
    .expect("window")
}

pub(super) fn run_item(cx: &mut TestAppContext, window: AnyWindowHandle, text: &str) {
    cx.update_window(window, |_, window, cx| window.input(text, cx))
        .expect("window");
    cx.run_until_parked();
    press_key(cx, window, "enter");
}

pub(super) fn press_key(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.press(key, cx);
    })
    .expect("window");
    cx.run_until_parked();
    redraw(cx, window);
}

fn body_has_focus(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<FerriteWindow>,
) -> bool {
    let focus = view.read_with(cx, |view, cx| {
        let body = view.viewer.as_ref()?.body.as_ref()?;
        Some(body.focus_handle(cx))
    });
    let Some(focus) = focus else {
        return false;
    };
    cx.update_window(window, |_, window, _| focus.is_focused(window))
        .expect("window")
}

pub(super) fn hint(
    cx: &mut TestAppContext,
    view: &Entity<FerriteWindow>,
    text: &str,
) -> Option<String> {
    view.read_with(cx, |view, cx| {
        let (menu, _) = view.viewer.as_ref()?.menu.as_ref()?;
        let items = &menu.read(cx).menu().items;
        let item = items.iter().find(|item| item.text == text)?;
        item.accelerator.clone()
    })
}

pub(super) fn clipboard(cx: &mut TestAppContext) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

pub(super) fn has(rows: &[String], text: &str) -> bool {
    rows.iter().any(|row| row == text)
}

#[gpui_kit::test]
fn a_right_click_in_f3_text_opens_the_menu_with_copy_and_no_paste(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(b"hello\nworld\n".to_vec()));
    assert!(rows(cx, window).is_empty());
    let at = middle(cx, window);
    right_click(cx, window, at);
    let shown = rows(cx, window);
    assert!(has(&shown, "Copy"), "{shown:?}");
    assert!(has(&shown, "Select all"), "{shown:?}");
    assert!(!has(&shown, "Paste"), "{shown:?}");
    assert!(!has(&shown, "Cut"), "{shown:?}");
    assert!(!shown.iter().any(|row| row == "# Windows"), "{shown:?}");
    assert_eq!(hint(cx, &view, "Bookmark").as_deref(), Some("Ctrl+F2"));
    assert_eq!(hint(cx, &view, "Copy").as_deref(), Some("Ctrl+C"));
}

#[gpui_kit::test]
fn the_f4_editor_menu_offers_cut_and_paste(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(b"hello\nworld\n".to_vec()));
    press(cx, window, &view, "ctrl-e");
    let at = middle(cx, window);
    right_click(cx, window, at);
    let shown = rows(cx, window);
    for item in ["Cut", "Copy", "Paste", "Select all"] {
        assert!(has(&shown, item), "{item} in {shown:?}");
    }
}

#[gpui_kit::test]
fn shift_f10_opens_the_menu_at_the_text_cursor(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let (window, view, _) = text_window(cx, Setup::new(b"hello\nworld\nthird\n".to_vec()));
    press(cx, window, &view, "down");
    let editor = editor_of(cx, &view).expect("the text editor is open");
    let cursor = editor
        .read_with(cx, |state, _| {
            let at = state.cursor();
            state.range_to_bounds(&(at..at))
        })
        .expect("the cursor is laid out");
    assert!(cursor.origin.y > px(0.));
    press_key(cx, window, "shift-f10");
    assert!(has(&rows(cx, window), "Copy"));
    let menu = menu_bounds(cx, window);
    let gap = menu.origin - cursor.bottom_left();
    assert!(gap.x.abs() < px(1.) && gap.y.abs() < px(1.), "{gap:?}");
}

#[gpui_kit::test]
fn the_menu_key_opens_the_viewer_menu_not_the_file_list_menu(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(b"hello\n".to_vec()));
    press_key(cx, window, "menu");
    assert!(has(&rows(cx, window), "Select all"));
    assert!(view.read_with(cx, |view, _| view.menu_run.is_none()));
}

#[gpui_kit::test]
fn copy_path_in_the_tree_menu_copies_the_node_path(cx: &mut TestAppContext) {
    let source = br#"{"store": {"book": []}, "name": "shop"}"#;
    let (window, view, _) = open_named(cx, "shop.json", Setup::new(source.to_vec()));
    press(cx, window, &view, "down");
    let at = tree_row_at(cx, window, 2);
    right_click(cx, window, at);
    let shown = rows(cx, window);
    for item in ["Copy path", "Fold branch", "Fold all", "Depth"] {
        assert!(has(&shown, item), "{item} in {shown:?}");
    }
    run_item(cx, window, "copy path");
    assert_eq!(clipboard(cx).as_deref(), Some("$.name"));
    assert!(rows(cx, window).is_empty());
}

#[gpui_kit::test]
fn copy_cell_in_the_table_menu_copies_the_cell(cx: &mut TestAppContext) {
    let source = b"name,qty\nPear,10\nApple,9\n";
    let (window, view, _) = open_named(cx, "fruit.csv", Setup::new(source.to_vec()));
    press(cx, window, &view, "right");
    let at = table_cell_at(cx, window, (1, 0));
    right_click(cx, window, at);
    let shown = rows(cx, window);
    for item in ["Copy cell", "Copy row", "Sort by this column"] {
        assert!(has(&shown, item), "{item} in {shown:?}");
    }
    run_item(cx, window, "copy cell");
    assert_eq!(clipboard(cx).as_deref(), Some("Apple"));
}

#[gpui_kit::test]
fn a_right_click_on_a_markdown_link_offers_open_link(cx: &mut TestAppContext) {
    let source = format!("[{LINK} {LINK} {LINK}](https://example.com)\n");
    let (window, _, _) = open_named(cx, "notes.md", Setup::new(source.into_bytes()));
    let block = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.find(("md-block", 0usize)).bounds()
        })
        .expect("window");
    let at = block.center();
    right_click(cx, window, at);
    assert_eq!(rows(cx, window), ["Copy", "-", "Open link", "Copy link"]);
}

#[gpui_kit::test]
fn focus_returns_to_the_body_after_an_item_runs(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(b"hello\nworld\n".to_vec()));
    let at = middle(cx, window);
    right_click(cx, window, at);
    assert!(!body_has_focus(cx, window, &view));
    run_item(cx, window, "select all");
    assert!(rows(cx, window).is_empty());
    assert!(body_has_focus(cx, window, &view));
    let editor = editor_of(cx, &view).expect("the text editor is open");
    let selected = editor.read_with(cx, |state, _| state.selected_range());
    assert_eq!(selected, 0.."hello\nworld\n".len());
}

#[gpui_kit::test]
fn escape_closes_the_menu_and_keeps_the_viewer(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(b"hello\n".to_vec()));
    let at = middle(cx, window);
    right_click(cx, window, at);
    assert!(!rows(cx, window).is_empty());
    press_key(cx, window, "escape");
    assert!(rows(cx, window).is_empty());
    assert!(viewer_open(cx, &view));
    assert!(body_has_focus(cx, window, &view));
}
