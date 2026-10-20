use std::ops::Range;
use std::rc::Rc;

use gpui_kit::{
    AnyElement, App, ClickEvent, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    StatefulInteractiveElement, Window,
};
use rusty_core::{Command, CursorMove};

use super::row::{self, RowLook};
use super::{FileListAction, ListContext, lasso, press, row_number};
use crate::tokens::{self, Theme};

fn render_row(ctx: &Rc<ListContext>, theme: &Theme, index: usize) -> Option<AnyElement> {
    let entry = ctx.props.rows.get(index)?;
    let row = row_number(index);
    let look = RowLook {
        theme,
        entry,
        is_cursor: ctx.props.view.cursor == row,
        is_marked: ctx.props.marked.is_marked(row),
        is_active_pane: ctx.props.is_active,
        parts: ctx.parts,
    };
    let (left, right, click) = (ctx.clone(), ctx.clone(), ctx.clone());
    let element = row::cells(&look, lasso::start_zone(ctx, row))
        .id("cells")
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            left.left_down((row, event), window, cx);
        })
        .on_mouse_down(MouseButton::Right, move |event, window, cx| {
            cx.stop_propagation();
            right.right_down((row, event.position), window, cx);
        })
        .on_mouse_move(press::drag_over(ctx, row))
        .on_click(move |event, window, cx| click.modified_click((row, event), window, cx));
    Some(element.into_any_element())
}

pub(super) fn render_rows(
    ctx: &Rc<ListContext>,
) -> impl Fn(Range<usize>, &mut Window, &mut App) -> Vec<AnyElement> + 'static {
    let ctx = ctx.clone();
    move |range, _, cx| {
        let theme = tokens::current(cx);
        range
            .filter_map(|index| render_row(&ctx, theme, index))
            .collect()
    }
}

impl ListContext {
    fn left_down(&self, (row, event): (u64, &MouseDownEvent), window: &mut Window, cx: &mut App) {
        self.state.update(cx, |state, _| state.lasso.press());
        let modifiers = event.modifiers;
        if modifiers.control || modifiers.shift {
            self.take_focus(window, cx);
            return;
        }
        self.click(row, window, cx);
        if event.click_count >= 2 {
            self.act(FileListAction::Open(row), window, cx);
        }
    }

    fn modified_click(&self, (row, event): (u64, &ClickEvent), window: &mut Window, cx: &mut App) {
        if self.state.read(cx).lasso.swallows_click() {
            return;
        }
        let modifiers = event.modifiers();
        let to = CursorMove::Row(row);
        if modifiers.shift {
            let unmark = false;
            self.send(Command::SelectionExtend { to, unmark }, window, cx);
        } else if modifiers.control {
            self.send(Command::PaneCursorMove { to }, window, cx);
            self.send(Command::SelectionToggle, window, cx);
        }
    }
}
