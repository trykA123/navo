use std::rc::Rc;
use std::time::Duration;

use gpui_kit::{
    App, Div, InteractiveElement, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point,
    Stateful, Window, point,
};
use rusty_core::{Command, CursorMove, SpanMode};

use super::row::check_column;
use super::{FileListAction, ListContext, row_index, row_number};
use crate::tokens;

const LONG_PRESS: Duration = Duration::from_millis(400);

#[derive(Clone, Copy)]
struct Held {
    press: u64,
    row: u64,
    last: u64,
    mode: SpanMode,
    dragged: bool,
}

#[derive(Default)]
pub(super) struct Press {
    held: Option<Held>,
    count: u64,
}

impl Press {
    fn start(&mut self, row: u64, mode: SpanMode) -> u64 {
        self.count += 1;
        self.held = Some(Held {
            press: self.count,
            row,
            last: row,
            mode,
            dragged: false,
        });
        self.count
    }

    fn fire(&mut self, press: u64) -> Option<u64> {
        let held = self
            .held
            .filter(|held| held.press == press && !held.dragged)?;
        self.held = None;
        Some(held.row)
    }

    fn drag(&mut self, row: u64) -> Option<Held> {
        let held = self.held.as_mut().filter(|held| held.last != row)?;
        let before = *held;
        held.dragged = true;
        held.last = row;
        Some(before)
    }

    fn release(&mut self) -> bool {
        self.held.take().is_some_and(|held| !held.dragged)
    }
}

pub(super) fn drag_over(
    ctx: &Rc<ListContext>,
    row: u64,
) -> impl Fn(&MouseMoveEvent, &mut Window, &mut App) + 'static {
    let ctx = ctx.clone();
    move |event, window, cx| {
        if event.pressed_button != Some(MouseButton::Right) {
            return;
        }
        let Some(held) = ctx.state.update(cx, |state, _| state.press.drag(row)) else {
            return;
        };
        if !held.dragged {
            ctx.act(FileListAction::MenuCancel, window, cx);
            let to = CursorMove::Row(held.row);
            ctx.send(Command::PaneCursorMove { to }, window, cx);
        }
        let (to, mode) = (CursorMove::Row(row), held.mode);
        ctx.send(Command::SelectionExtend { to, mode }, window, cx);
    }
}

pub(super) fn background_down(
    ctx: &Rc<ListContext>,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
    let ctx = ctx.clone();
    move |event, window, cx| {
        ctx.take_focus(window, cx);
        let action = FileListAction::BackgroundMenu {
            at: event.position,
            extended: event.modifiers.shift,
        };
        ctx.act(action, window, cx);
    }
}

pub(super) fn release_on(root: Stateful<Div>, ctx: &Rc<ListContext>) -> Stateful<Div> {
    let (inside, outside) = (ctx.clone(), ctx.clone());
    root.on_mouse_up(MouseButton::Right, move |_, window, cx| {
        inside.right_up(window, cx);
    })
    .on_mouse_up_out(MouseButton::Right, move |_, window, cx| {
        outside.right_up(window, cx);
    })
}

impl ListContext {
    pub(super) fn right_down(
        self: &Rc<Self>,
        (row, at, extended): (u64, Point<Pixels>, bool),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.click(row, window, cx);
        let mode = if self.props.marked.is_marked(row) {
            SpanMode::Unmark
        } else {
            SpanMode::Mark
        };
        let press = self
            .state
            .update(cx, |state, _| state.press.start(row, mode));
        let adds_cursor = self.adds_pressed_row(row);
        let prefetch = FileListAction::MenuPrefetch {
            adds_cursor,
            extended,
        };
        self.act(prefetch, window, cx);
        let ctx = self.clone();
        window
            .spawn(cx, async move |cx| {
                cx.background_executor().timer(LONG_PRESS).await;
                let _ = cx.update(|window, cx| ctx.long_press((press, at, extended), window, cx));
            })
            .detach();
    }

    fn long_press(
        &self,
        (press, at, extended): (u64, Point<Pixels>, bool),
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(row) = self.state.update(cx, |state, _| state.press.fire(press)) else {
            return;
        };
        let to = CursorMove::Row(row);
        self.send(Command::PaneCursorMove { to }, window, cx);
        if self.adds_pressed_row(row) {
            self.send(Command::SelectionToggle, window, cx);
        }
        self.act(FileListAction::ContextMenu { at, extended }, window, cx);
    }

    fn right_up(&self, window: &mut Window, cx: &mut App) {
        if self.state.update(cx, |state, _| state.press.release()) {
            self.act(FileListAction::MenuCancel, window, cx);
            self.send(Command::SelectionToggle, window, cx);
        }
    }

    fn adds_pressed_row(&self, row: u64) -> bool {
        let marked = &self.props.marked;
        marked.count() > 0 && !marked.is_marked(row)
    }

    pub(super) fn menu_at_cursor(&self, window: &mut Window, cx: &mut App) {
        let anchor = self.cursor_anchor(cx);
        let on_row = self.props.rows.get(row_index(self.props.view.cursor));
        let action = match on_row {
            Some(_) => FileListAction::ContextMenu {
                at: anchor,
                extended: false,
            },
            None => FileListAction::BackgroundMenu {
                at: anchor,
                extended: false,
            },
        };
        self.act(action, window, cx);
    }

    fn cursor_anchor(&self, cx: &App) -> Point<Pixels> {
        let t = tokens::current(cx);
        let state = self.state.read(cx);
        let Some(list) = state.list_bounds else {
            return Point::default();
        };
        let row_height = t.spacing.row_height;
        let top = row_number(state.scroll.top_row(row_height));
        let below = self.props.view.cursor.saturating_sub(top) + 1;
        let y = list.origin.y - state.lift + row_height * (below as f32);
        let name = t.spacing.xs + t.spacing.lg + check_column(t) + t.spacing.md;
        point(list.origin.x + name, y.min(list.bottom()))
    }
}
