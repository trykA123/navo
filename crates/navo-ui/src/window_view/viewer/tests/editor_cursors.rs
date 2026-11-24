//! F3 editor multi-cursor tests: Ctrl+Alt+Down adds a cursor that types and undoes in sync,
//! an Alt+Shift drag makes a box selection (the widget's own gesture, no Navo code), and
//! Alt+Z or the View menu turns soft wrap of the edit mode off and on.

use gpui_kit::{AnyWindowHandle, Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext};

use super::menu::run_item;
use super::view_rig::{Rig, rig};
use super::{Setup, editor_of, editor_text, redraw, text_window};
use crate::test_pointer::{dispatch, drag, up};
use crate::window_view::NavoWindow;
use crate::window_view::tests::{press, type_in};

const LINES: &str = "abcd\nefgh\nijkl\nmnop\n";

/// Opens `LINES` in the F3 editor and turns editing on; returns the window and its view.
fn editing(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<NavoWindow>) {
    let (window, view, _) = text_window(cx, Setup::new(LINES.as_bytes().to_vec()));
    press(cx, window, &view, "ctrl-e");
    redraw(cx, window);
    (window, view)
}

/// The window position of byte `offset` in the editor, at the middle of its line.
fn point_of(cx: &mut TestAppContext, view: &Entity<NavoWindow>, offset: usize) -> Point<Pixels> {
    let editor = editor_of(cx, view).expect("the text editor is open");
    let bounds = editor
        .read_with(cx, |state, _| state.range_to_bounds(&(offset..offset)))
        .expect("the offset is laid out");
    Point::new(bounds.origin.x, bounds.center().y)
}

#[gpui_kit::test]
fn ctrl_alt_down_adds_a_cursor_that_types_and_undoes_with_the_first(cx: &mut TestAppContext) {
    let (window, view) = editing(cx);
    press(cx, window, &view, "ctrl-alt-down");
    type_in(cx, window, &view, "x");
    assert_eq!(editor_text(cx, &view), "xabcd\nxefgh\nijkl\nmnop\n");
    press(cx, window, &view, "ctrl-z");
    assert_eq!(editor_text(cx, &view), LINES);
}

#[gpui_kit::test]
fn ctrl_alt_up_adds_a_cursor_on_the_line_above(cx: &mut TestAppContext) {
    let (window, view) = editing(cx);
    press(cx, window, &view, "down");
    press(cx, window, &view, "ctrl-alt-up");
    type_in(cx, window, &view, "y");
    assert_eq!(editor_text(cx, &view), "yabcd\nyefgh\nijkl\nmnop\n");
}

#[gpui_kit::test]
fn an_alt_shift_drag_over_three_lines_selects_the_same_columns_on_each(cx: &mut TestAppContext) {
    let (window, view) = editing(cx);
    let (from, to) = (point_of(cx, &view, 1), point_of(cx, &view, 13));
    let box_drag = Modifiers {
        alt: true,
        shift: true,
        ..Modifiers::default()
    };
    drag(cx, window, (MouseButton::Left, box_drag), &[from, to]);
    dispatch(cx, window, up(MouseButton::Left, to, box_drag));
    type_in(cx, window, &view, "Z");
    assert_eq!(editor_text(cx, &view), "aZd\neZh\niZl\nmnop\n");
}

/// Opens one short and one 600-character line in the F3 editor and turns editing on.
fn long_line_editing(cx: &mut TestAppContext) -> Rig {
    let text = format!("short\n{}\n", "word ".repeat(120));
    let rig = rig(cx, ("a.txt", "b.txt"), text.as_bytes());
    rig.press(cx, "ctrl-e");
    redraw(cx, rig.window);
    rig
}

/// Whether the open editor soft-wraps its lines.
fn wraps(cx: &mut TestAppContext, rig: &Rig) -> bool {
    redraw(cx, rig.window);
    let editor = rig.editor(cx);
    editor.read_with(cx, |editor, _| editor.wraps())
}

/// The number of visible rows the editor lays its text out in.
fn visible_rows(cx: &mut TestAppContext, rig: &Rig) -> usize {
    redraw(cx, rig.window);
    let editor = editor_of(cx, &rig.view).expect("the text editor is open");
    editor.read_with(cx, |state, _| state.display_map().wrap_row_count())
}

#[gpui_kit::test]
fn alt_z_turns_soft_wrap_off_and_on_again_while_editing(cx: &mut TestAppContext) {
    let rig = long_line_editing(cx);
    assert!(wraps(cx, &rig));
    rig.press(cx, "alt-z");
    assert!(!wraps(cx, &rig));
    rig.press(cx, "alt-z");
    assert!(wraps(cx, &rig));
}

#[gpui_kit::test]
fn a_long_line_takes_more_visible_rows_with_soft_wrap_on(cx: &mut TestAppContext) {
    let rig = long_line_editing(cx);
    let wrapped = visible_rows(cx, &rig);
    rig.press(cx, "alt-z");
    let unwrapped = visible_rows(cx, &rig);
    assert!(
        wrapped > unwrapped,
        "{wrapped} rows wrapped, {unwrapped} unwrapped"
    );
    rig.press(cx, "alt-z");
    assert_eq!(visible_rows(cx, &rig), wrapped);
}

#[gpui_kit::test]
fn the_view_menu_soft_wrap_row_shows_and_changes_the_state(cx: &mut TestAppContext) {
    let rig = long_line_editing(cx);
    rig.press(cx, "alt-v");
    assert!(rig.is_checked(cx, "Soft wrap"), "{:?}", rig.rows(cx));
    run_item(cx, rig.window, "Soft wrap");
    let rows = rig.rows(cx);
    assert!(rows.contains(&("Soft wrap".to_owned(), false)), "{rows:?}");
    assert!(!wraps(cx, &rig));
}
