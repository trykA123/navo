use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, Modifiers, MouseButton, Pixels,
    PlatformInput, Point, TestAppContext, point, px,
};
use rusty_core::{AppState, PaneId};
use rusty_fs::FakeFs;

use super::{open_options, options_for, press, settle};
use crate::test_pointer::{self, ctrl, down, moved, shift, up};
use crate::window_view::FerriteWindow;

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    pane: PaneId,
}

fn rig(cx: &mut TestAppContext) -> Rig {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    for name in ["a.txt", "b.txt", "c.md", "d.txt"] {
        fs.add_file(&format!("home/{name}"), 1);
    }
    let fs = Arc::new(fs);
    let state = AppState::new(fs.location("home"));
    let (window, view) = open_options(cx, options_for(fs, state));
    let pane = view.read_with(cx, |view, _| view.browser.state().focused_pane());
    Rig { window, view, pane }
}

impl Rig {
    fn press(&self, cx: &mut TestAppContext, keys: &[&str]) {
        for key in keys {
            press(cx, self.window, &self.view, key);
        }
    }

    fn send(&self, cx: &mut TestAppContext, input: PlatformInput) {
        test_pointer::dispatch(cx, self.window, input);
        settle(cx, &self.view);
    }

    fn row(&self, cx: &mut TestAppContext, row: usize) -> Bounds<Pixels> {
        let pane = self.pane;
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window
                .within(("list", pane.get()))
                .find(("row", row))
                .bounds()
        })
        .expect("window")
    }

    fn click(&self, cx: &mut TestAppContext, row: usize, modifiers: Modifiers) {
        let at = self.row(cx, row).center();
        self.send(cx, down(MouseButton::Left, at, modifiers));
        self.send(cx, up(MouseButton::Left, at, modifiers));
    }

    fn below(&self, cx: &mut TestAppContext) -> Point<Pixels> {
        let last = self.row(cx, 4);
        point(last.center().x, last.bottom() + px(60.))
    }

    fn drag(&self, cx: &mut TestAppContext, button: MouseButton, path: &[Point<Pixels>]) {
        let [from, rest @ ..] = path else {
            return;
        };
        self.send(cx, down(button, *from, Modifiers::default()));
        for at in rest {
            self.send(cx, moved(Some(button), *at, Modifiers::default()));
        }
    }

    fn right_drag(&self, cx: &mut TestAppContext, rows: &[usize]) {
        let path: Vec<_> = rows.iter().map(|row| self.row(cx, *row).center()).collect();
        self.drag(cx, MouseButton::Right, &path);
    }

    fn marked(&self, cx: &mut TestAppContext) -> Vec<u64> {
        self.view.read_with(cx, |view, _| {
            view.browser.state().active_tab().selection().marked()
        })
    }

    fn cursor(&self, cx: &mut TestAppContext) -> u64 {
        self.view.read_with(cx, |view, _| {
            view.browser.state().active_tab().view().cursor
        })
    }
}

#[gpui_kit::test]
fn ctrl_click_marks_two_rows_and_a_second_ctrl_click_unmarks(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.click(cx, 1, ctrl());
    rig.click(cx, 3, ctrl());
    assert_eq!(rig.marked(cx), [1, 3]);
    assert_eq!(rig.cursor(cx), 3);
    rig.click(cx, 1, ctrl());
    assert_eq!(rig.marked(cx), [3]);
}

#[gpui_kit::test]
fn shift_click_marks_the_range_from_the_cursor(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, &["down"]);
    rig.click(cx, 3, shift());
    assert_eq!(rig.marked(cx), [1, 2, 3]);
    rig.click(cx, 2, shift());
    assert_eq!(rig.marked(cx), [1, 2]);
}

#[gpui_kit::test]
fn a_click_below_the_last_row_unmarks_all_and_slash_restores(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.click(cx, 1, ctrl());
    rig.click(cx, 2, ctrl());
    let at = rig.below(cx);
    rig.send(cx, down(MouseButton::Left, at, Modifiers::default()));
    rig.send(cx, up(MouseButton::Left, at, Modifiers::default()));
    assert_eq!(rig.marked(cx), Vec::<u64>::new());
    rig.press(cx, &["/"]);
    assert_eq!(rig.marked(cx), [1, 2]);
}

#[gpui_kit::test]
fn a_right_drag_marks_a_range_and_shrinks_back(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.right_drag(cx, &[1, 3]);
    assert_eq!(rig.marked(cx), [1, 2, 3]);
    let row_two = rig.row(cx, 2).center();
    rig.send(
        cx,
        moved(Some(MouseButton::Right), row_two, Modifiers::default()),
    );
    assert_eq!(rig.marked(cx), [1, 2]);
    rig.send(cx, up(MouseButton::Right, row_two, Modifiers::default()));
    assert_eq!(rig.marked(cx), [1, 2]);
}

#[gpui_kit::test]
fn a_right_drag_from_a_marked_row_unmarks(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, &["shift-end"]);
    rig.right_drag(cx, &[3, 1]);
    assert_eq!(rig.marked(cx), [0, 4]);
}

#[gpui_kit::test]
fn a_lasso_marks_the_crossed_rows_and_keeps_earlier_marks(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.click(cx, 0, ctrl());
    let start = rig.below(cx);
    let row_two = rig.row(cx, 2).center();
    let row_three = rig.row(cx, 3).center();
    rig.drag(cx, MouseButton::Left, &[start, row_two]);
    assert_eq!(rig.marked(cx), [0, 2, 3, 4]);
    rig.send(
        cx,
        moved(Some(MouseButton::Left), row_three, Modifiers::default()),
    );
    assert_eq!(rig.marked(cx), [0, 3, 4]);
    rig.send(cx, up(MouseButton::Left, row_three, Modifiers::default()));
    assert_eq!(rig.marked(cx), [0, 3, 4]);
}
