//! F3 editor multi-cursor tests: Ctrl+Alt+Down adds a cursor that types and undoes in sync,
//! and an Alt+Shift drag makes a box selection (the widget's own gesture, no Navo code).

use gpui_kit::{AnyWindowHandle, Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext};

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
