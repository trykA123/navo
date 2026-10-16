use std::rc::Rc;
use std::time::Duration;

use gpui_kit::{App, Div, InteractiveElement, MouseButton, Pixels, Point, Stateful, Window, point};
use rusty_core::Command;

use super::row::check_column;
use super::{FileListAction, ListContext, row_index, row_number};
use crate::tokens;

const LONG_PRESS: Duration = Duration::from_millis(400);

#[derive(Default)]
pub(super) struct Press {
    held: Option<u64>,
    count: u64,
}

impl Press {
    fn start(&mut self) -> u64 {
        self.count += 1;
        self.held = Some(self.count);
        self.count
    }

    fn fire(&mut self, press: u64) -> bool {
        let fired = self.held == Some(press);
        if fired {
            self.held = None;
        }
        fired
    }

    fn release(&mut self) -> bool {
        self.held.take().is_some()
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
        (row, at): (u64, Point<Pixels>),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.click(row, window, cx);
        let press = self.state.update(cx, |state, _| state.press.start());
        let ctx = self.clone();
        window
            .spawn(cx, async move |cx| {
                cx.background_executor().timer(LONG_PRESS).await;
                let _ = cx.update(|window, cx| ctx.long_press((press, at), window, cx));
            })
            .detach();
    }

    fn long_press(&self, (press, at): (u64, Point<Pixels>), window: &mut Window, cx: &mut App) {
        if self.state.update(cx, |state, _| state.press.fire(press)) {
            self.act(FileListAction::ContextMenu(at), window, cx);
        }
    }

    fn right_up(&self, window: &mut Window, cx: &mut App) {
        if self.state.update(cx, |state, _| state.press.release()) {
            self.send(Command::SelectionToggle, window, cx);
        }
    }

    pub(super) fn menu_at_cursor(&self, window: &mut Window, cx: &mut App) {
        if self
            .props
            .rows
            .get(row_index(self.props.view.cursor))
            .is_some()
        {
            let anchor = self.cursor_anchor(cx);
            self.act(FileListAction::ContextMenu(anchor), window, cx);
        }
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
        let y = list.origin.y + row_height * (below as f32);
        let name = t.spacing.xs + t.spacing.lg + check_column(t) + t.spacing.md;
        point(list.origin.x + name, y.min(list.bottom()))
    }
}
