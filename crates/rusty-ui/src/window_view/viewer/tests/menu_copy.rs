use gpui_kit::component::input::Position;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, ClipboardEntry, Entity, TestAppContext};
use image::ImageFormat;

use super::image::{open_first, rig};
use super::menu::{
    clipboard, has, hint, middle, press_key, right_click, rows, run_item, table_cell_at,
    table_head_at, tree_row_at,
};
use super::structured::open_named;
use super::{Setup, editor_of, label, redraw, text_of, text_window};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{press, settle};
use crate::window_view::viewer::LINE_BREAK;
use crate::window_view::viewer::editor::EditorBody;
use crate::window_view::viewer::image::fixtures::encoded;

const SHORTENED: &str =
    "This value is shortened in the tree. Press 1 to view the text and copy it from there.";
const LINES: &[u8] = b"hello world\nsecond line\nthird\nfourth\nfifth\n";

fn location(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> String {
    view.read_with(cx, |view, _| {
        view.viewer_location()
            .map(|location| location.as_str().to_owned())
            .unwrap_or_default()
    })
}

fn find_text(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Option<String> {
    view.read_with(cx, |view, cx| {
        let body = view.viewer.as_ref()?.body.as_ref()?;
        let editor = body.as_any().downcast_ref::<EditorBody>()?;
        editor.find_text(cx)
    })
}

fn cursor_line(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> u32 {
    let editor = editor_of(cx, view).expect("the text editor is open");
    editor.read_with(cx, |state, _| state.cursor_position().line)
}

fn open_menu(cx: &mut TestAppContext, window: AnyWindowHandle) -> Vec<String> {
    let at = middle(cx, window);
    right_click(cx, window, at);
    rows(cx, window)
}

fn md_block(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    index: usize,
) -> gpui_kit::Point<gpui_kit::Pixels> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.find(("md-block", index)).bounds().center()
    })
    .expect("window")
}

#[gpui_kit::test]
fn find_selection_opens_find_with_the_word_at_the_cursor(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(LINES.to_vec()));
    let shown = open_menu(cx, window);
    assert!(has(&shown, "Find selection"), "{shown:?}");
    assert_eq!(
        hint(cx, &view, "Find selection").as_deref(),
        Some("Ctrl+F3")
    );
    assert_eq!(
        hint(cx, &view, "Copy path:line").as_deref(),
        Some("Ctrl+Shift+C")
    );
    assert_eq!(hint(cx, &view, "Go to line").as_deref(), Some("Ctrl+G"));
    run_item(cx, window, "find selection");
    assert_eq!(find_text(cx, &view).as_deref(), Some("hello"));
}

#[gpui_kit::test]
fn find_selection_opens_find_with_the_selected_text(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(LINES.to_vec()));
    let editor = editor_of(cx, &view).expect("the text editor is open");
    editor.update(cx, |state, cx| state.set_selected_range(12..18, cx));
    open_menu(cx, window);
    run_item(cx, window, "find selection");
    assert_eq!(find_text(cx, &view).as_deref(), Some("second"));
}

#[gpui_kit::test]
fn find_selection_in_f4_fills_the_editor_search(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(LINES.to_vec()));
    press(cx, window, &view, "ctrl-e");
    press_key(cx, window, "ctrl-f3");
    let editor = editor_of(cx, &view).expect("the text editor is open");
    let query = editor.read_with(cx, |state, _| state.search_session().query.clone());
    assert_eq!(query, "hello");
}

#[gpui_kit::test]
fn copy_path_line_copies_the_path_and_the_cursor_line(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(LINES.to_vec()));
    press(cx, window, &view, "down");
    press(cx, window, &view, "down");
    open_menu(cx, window);
    run_item(cx, window, "copy path");
    let expected = format!("{}:3", location(cx, &view));
    assert_eq!(clipboard(cx), Some(expected));
}

#[gpui_kit::test]
fn go_to_line_from_the_editor_menu_moves_the_cursor(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(LINES.to_vec()));
    assert_eq!(cursor_line(cx, &view), 0);
    let shown = open_menu(cx, window);
    assert!(has(&shown, "Go to line"), "{shown:?}");
    run_item(cx, window, "go to line");
    cx.update_window(window, |_, window, cx| window.input("4", cx))
        .expect("window");
    press_key(cx, window, "enter");
    assert_eq!(cursor_line(cx, &view), 3);
    let editor = editor_of(cx, &view).expect("the text editor is open");
    let at = editor.read_with(cx, |state, _| state.cursor_position());
    assert_eq!(at, Position::new(3, 0));
}

#[gpui_kit::test]
fn go_to_line_works_while_editing(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(LINES.to_vec()));
    press(cx, window, &view, "ctrl-e");
    press_key(cx, window, "ctrl-g");
    cx.update_window(window, |_, window, cx| window.input("2", cx))
        .expect("window");
    press_key(cx, window, "enter");
    assert_eq!(cursor_line(cx, &view), 1);
}

#[gpui_kit::test]
fn copy_value_and_copy_key_in_the_tree_menu(cx: &mut TestAppContext) {
    let source = br#"{"store": {"book": [1, "two"]}, "name": "shop"}"#;
    let (window, view, _) = open_named(cx, "shop.json", Setup::new(source.to_vec()));
    press(cx, window, &view, "down");
    let shown = open_menu(cx, window);
    for item in ["Copy value", "Copy key", "Copy path"] {
        assert!(has(&shown, item), "{item} in {shown:?}");
    }
    run_item(cx, window, "copy value");
    assert_eq!(clipboard(cx).as_deref(), Some(r#"{"book":[1,"two"]}"#));
    open_menu(cx, window);
    run_item(cx, window, "copy key");
    assert_eq!(clipboard(cx).as_deref(), Some("store"));
}

#[gpui_kit::test]
fn copy_column_in_the_table_menu_copies_the_header_and_the_values(cx: &mut TestAppContext) {
    let source = b"name,qty\nPear,10\nApple,9\n";
    let (window, view, _) = open_named(cx, "fruit.csv", Setup::new(source.to_vec()));
    press(cx, window, &view, "right");
    let shown = open_menu(cx, window);
    assert!(has(&shown, "Copy column"), "{shown:?}");
    run_item(cx, window, "copy column");
    let expected = ["qty", "10", "9"].join(LINE_BREAK);
    assert_eq!(clipboard(cx), Some(expected));
}

#[gpui_kit::test]
fn copy_in_the_markdown_menu_copies_the_block_under_the_pointer(cx: &mut TestAppContext) {
    let source = b"# Title\n\nFirst paragraph.\n\nSecond paragraph.\n".to_vec();
    let (window, _, _) = open_named(cx, "notes.md", Setup::new(source));
    let at = md_block(cx, window, 1);
    right_click(cx, window, at);
    assert_eq!(rows(cx, window), ["# Ferrite", "Copy"]);
    run_item(cx, window, "copy");
    assert_eq!(clipboard(cx).as_deref(), Some("First paragraph."));
}

#[gpui_kit::test]
fn copy_link_in_the_markdown_menu_copies_the_link_target(cx: &mut TestAppContext) {
    let source = b"See [the docs](https://example.com/docs) here.\n".to_vec();
    let (window, view, _) = open_named(cx, "links.md", Setup::new(source));
    press(cx, window, &view, "tab");
    press_key(cx, window, "menu");
    assert_eq!(
        rows(cx, window),
        ["# Ferrite", "Copy", "-", "Open link", "Copy link"]
    );
    run_item(cx, window, "copy link");
    assert_eq!(clipboard(cx).as_deref(), Some("https://example.com/docs"));
}

#[gpui_kit::test]
fn copy_path_in_the_image_menu_copies_the_file_path(cx: &mut TestAppContext) {
    let image = rig(cx, &[], &[("a.txt", encoded(ImageFormat::Png, 300, 200))]);
    open_first(cx, &image);
    let (window, view, _) = &image;
    let shown = open_menu(cx, *window);
    assert_eq!(
        shown,
        ["# Ferrite", "Copy image", "Copy path", "-", "Fit", "100 %"]
    );
    run_item(cx, *window, "copy path");
    let expected = location(cx, view);
    assert!(expected.ends_with("a.txt"), "{expected}");
    assert_eq!(clipboard(cx), Some(expected));
}

#[gpui_kit::test]
fn copy_image_puts_a_png_of_the_image_on_the_clipboard(cx: &mut TestAppContext) {
    let image = rig(cx, &[], &[("a.txt", encoded(ImageFormat::Png, 300, 200))]);
    open_first(cx, &image);
    let window = image.0;
    open_menu(cx, window);
    run_item(cx, window, "copy image");
    cx.run_until_parked();
    let item = cx
        .read_from_clipboard()
        .expect("the clipboard has the image");
    let Some(ClipboardEntry::Image(copied)) = item.entries().first() else {
        panic!("not an image: {item:?}");
    };
    assert_eq!(copied.format, gpui_kit::ImageFormat::Png);
    let decoded =
        image::load_from_memory_with_format(&copied.bytes, ImageFormat::Png).expect("a valid PNG");
    assert_eq!((decoded.width(), decoded.height()), (300, 200));
}

#[gpui_kit::test]
fn copy_value_refuses_a_value_the_tree_shortened(cx: &mut TestAppContext) {
    let source = format!("{{\"long\": \"{}\", \"short\": \"x\"}}", "a".repeat(400));
    let (window, view, _) = open_named(cx, "long.json", Setup::new(source.into_bytes()));
    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("before".to_owned()));
    press(cx, window, &view, "down");
    open_menu(cx, window);
    run_item(cx, window, "copy value");
    assert_eq!(clipboard(cx).as_deref(), Some("before"));
    assert_eq!(text_of(cx, window, "tree-status"), SHORTENED);
    press(cx, window, &view, "down");
    open_menu(cx, window);
    run_item(cx, window, "copy value");
    assert_eq!(clipboard(cx).as_deref(), Some("x"));
}

#[gpui_kit::test]
fn ctrl_c_after_an_escaped_markdown_menu_copies_the_top_block(cx: &mut TestAppContext) {
    let source = b"# Title\n\nFirst paragraph.\n\nSecond paragraph.\n".to_vec();
    let (window, _, _) = open_named(cx, "notes.md", Setup::new(source));
    let at = md_block(cx, window, 1);
    right_click(cx, window, at);
    assert!(has(&rows(cx, window), "Copy"));
    press_key(cx, window, "escape");
    assert!(rows(cx, window).is_empty());
    press_key(cx, window, "ctrl-c");
    assert_eq!(clipboard(cx).as_deref(), Some("Title"));
}

const LETTERS: &[u8] = br#"{"a": 1, "b": 2, "c": 3, "d": 4, "e": 5, "f": 6, "g": 7, "h": 8}"#;
const PRICES: &[u8] = b"name,qty,price\nPear,10,3\nApple,9,2\nPlum,4,7\n";

#[gpui_kit::test]
fn tree_copy_value_and_key_act_on_the_right_clicked_row(cx: &mut TestAppContext) {
    let (window, view, _) = open_named(cx, "letters.json", Setup::new(LETTERS.to_vec()));
    press(cx, window, &view, "down");
    press(cx, window, &view, "down");
    let at = tree_row_at(cx, window, 7);
    right_click(cx, window, at);
    run_item(cx, window, "copy value");
    assert_eq!(clipboard(cx).as_deref(), Some("7"));
    let at = tree_row_at(cx, window, 5);
    right_click(cx, window, at);
    run_item(cx, window, "copy key");
    assert_eq!(clipboard(cx).as_deref(), Some("e"));
}

#[gpui_kit::test]
fn table_copy_cell_and_column_act_on_the_right_clicked_cell(cx: &mut TestAppContext) {
    let (window, view, _) = open_named(cx, "prices.csv", Setup::new(PRICES.to_vec()));
    press(cx, window, &view, "right");
    let at = table_cell_at(cx, window, (2, 2));
    right_click(cx, window, at);
    run_item(cx, window, "copy cell");
    assert_eq!(clipboard(cx).as_deref(), Some("7"));
    let at = table_cell_at(cx, window, (1, 0));
    right_click(cx, window, at);
    run_item(cx, window, "copy column");
    let expected = ["name", "Pear", "Apple", "Plum"].join(LINE_BREAK);
    assert_eq!(clipboard(cx), Some(expected));
}

#[gpui_kit::test]
fn sort_by_this_column_sorts_the_right_clicked_column(cx: &mut TestAppContext) {
    let (window, view, _) = open_named(cx, "prices.csv", Setup::new(PRICES.to_vec()));
    press(cx, window, &view, "right");
    let at = table_cell_at(cx, window, (0, 2));
    right_click(cx, window, at);
    run_item(cx, window, "sort by this column");
    settle(cx, &view);
    redraw(cx, window);
    assert_eq!(label(cx, window, ("table-row", 0)), "2: Apple, 9, 2");
}

#[gpui_kit::test]
fn sort_by_this_column_from_a_header_sorts_that_column(cx: &mut TestAppContext) {
    let (window, view, _) = open_named(cx, "prices.csv", Setup::new(PRICES.to_vec()));
    let at = table_head_at(cx, window, 1);
    right_click(cx, window, at);
    run_item(cx, window, "sort by this column");
    settle(cx, &view);
    redraw(cx, window);
    assert_eq!(label(cx, window, ("table-row", 0)), "3: Plum, 4, 7");
}
