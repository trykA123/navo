use std::ops::Range;
use std::rc::Rc;

use gpui_kit::{
    AnyElement, App, ClickEvent, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    StatefulInteractiveElement, Window,
};
use rusty_core::{Command, CursorMove, SpanMode};

use super::row::{self, RowLook};
use super::{FileListAction, ListContext, drop, lasso, press, row_number};
use crate::name_match::Needle;
use crate::tokens::{self, Theme};

fn render_row(
    ctx: &Rc<ListContext>,
    (theme, needle): (&Theme, Option<&Needle>),
    index: usize,
) -> Option<AnyElement> {
    let entry = ctx.props.rows.get(index)?;
    let row = row_number(index);
    let look = RowLook {
        theme,
        entry,
        is_cursor: ctx.props.view.cursor == row,
        is_marked: ctx.props.marked.is_marked(row),
        is_active_pane: ctx.props.is_active,
        parts: ctx.parts,
        widths: ctx.widths,
        is_find: ctx.find_results.is_some(),
        needle,
    };
    let (left, right, click) = (ctx.clone(), ctx.clone(), ctx.clone());
    let zones = (drop::grip(ctx, row), lasso::start_zone(ctx, row));
    let element = row::cells(&look, zones)
        .id("cells")
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            left.left_down((row, event), window, cx);
        })
        .on_mouse_down(MouseButton::Right, move |event, window, cx| {
            cx.stop_propagation();
            right.right_down((row, event.position, event.modifiers.shift), window, cx);
        })
        .on_mouse_move(press::drag_over(ctx, row))
        .on_click(move |event, window, cx| click.modified_click((row, event), window, cx));
    Some(drop::target(element, ctx, (row, theme)).into_any_element())
}

pub(super) fn render_rows(
    ctx: &Rc<ListContext>,
) -> impl Fn(Range<usize>, &mut Window, &mut App) -> Vec<AnyElement> + 'static {
    let ctx = ctx.clone();
    move |range, _, cx| {
        let theme = tokens::current(cx);
        let needle = ctx.props.filter.as_deref().map(Needle::parse);
        range
            .filter_map(|index| render_row(&ctx, (theme, needle.as_ref()), index))
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
            let mode = SpanMode::Mark;
            self.send(Command::SelectionExtend { to, mode }, window, cx);
        } else if modifiers.control {
            self.send(Command::PaneCursorMove { to }, window, cx);
            self.send(Command::SelectionToggle, window, cx);
        }
    }
}
