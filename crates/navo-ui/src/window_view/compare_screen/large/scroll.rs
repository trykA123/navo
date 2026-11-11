//! Scrolling the large text body (D16). The top row is a `u64` window start plus the rows
//! scrolled inside a list of at most 262,144 rows, so `f32` pixel offsets stay exact at 10M
//! lines. Keys, the wheel and the scrollbar move it in event handlers, which also ask for lines.

use gpui_kit::{ScrollEvent, Stateful};

use super::super::FileScreen;
use super::LargeShown;
use crate::compare_diff::PairControl;
use crate::prelude::*;
use crate::tokens;
use crate::window_view::NavoWindow;
use crate::window_view::viewer::{GoEnd, GoStart, PageDown, PageUp, RowDown, RowUp};

impl LargeShown {
    /// The top row: the window's first row plus the whole rows scrolled inside it.
    pub(super) fn top(&self) -> u64 {
        self.bar.top()
    }

    /// Pixels scrolled into the top row.
    pub(super) fn sub(&self) -> f32 {
        self.bar.sub()
    }

    /// Rows the list showed in the last layout, or 100 before the first one.
    pub(super) fn visible(&self) -> u64 {
        self.bar.visible()
    }

    /// The rows the list holds, from the window's first row.
    pub(super) fn window_len(&self) -> u64 {
        self.bar.window_len()
    }

    /// Puts row `row` on top with `sub` pixels scrolled into it; the window moves to centre
    /// the row when it nears an edge the file goes past.
    pub(super) fn place(&mut self, row: u64, sub: f32) {
        self.bar.put(row, sub);
    }

    /// Moves the top row by `rows`, up when negative.
    fn scroll_by(&mut self, rows: i64) {
        self.place(self.top().saturating_add_signed(rows), 0.);
    }

    /// Rows a page moves: the rows in view less one.
    fn page(&self) -> i64 {
        i64::try_from(self.visible().saturating_sub(1).max(1)).unwrap_or(i64::MAX)
    }

    /// Puts the last page in view.
    fn go_end(&mut self) {
        self.place(self.map.count().saturating_sub(self.visible()), 0.);
    }

    /// Follows a wheel or scrollbar move of the list: re-centres the window when needed.
    fn follow(&mut self) {
        let (top, sub) = (self.top(), self.sub());
        self.place(top, sub);
    }

    /// Asks `control` for the first missing window of the rows in view unless one is in flight,
    /// and moves the diff's focus to the middle row's line so its section is diffed first.
    pub(super) fn ask(&self, control: &dyn PairControl) {
        let top = self.top();
        let rows = top..top.saturating_add(self.visible()).min(self.map.count());
        let wanted = self.lines.borrow().wanted(&self.map, rows.clone());
        let Some(window) = wanted else {
            return;
        };
        self.lines.borrow_mut().asked(window.clone());
        let middle = self.map.at(rows.start + (rows.end - rows.start) / 2);
        control.focus(middle.left.or(middle.right).unwrap_or(window.start));
        control.lines(window);
    }

    /// The first line in view on each side, else `start`; a new window trims around them.
    pub(super) fn near(&self, start: u64) -> (u64, u64) {
        let top = self.top();
        let rows = top..top.saturating_add(self.visible()).min(self.map.count());
        let (left, right) = super::lines::in_view(&self.map, rows);
        let first = |range: Option<std::ops::Range<u64>>| range.map_or(start, |range| range.start);
        (first(left), first(right))
    }
}

impl FileScreen {
    /// Shows the large body in the picked layout, else in the window's (`auto`).
    pub(in crate::window_view::compare_screen) fn follow_layout(&mut self) {
        let layout = self.look.layout.unwrap_or(self.auto);
        if let Some(large) = self.large.as_mut() {
            large.relayout(layout);
            large.ask(self.control.as_ref());
        }
    }
}

impl NavoWindow {
    /// Runs `step` on the open large body, then asks for the lines now in view. With no large
    /// body the key goes on, so other file bodies and the list of changes still get it.
    fn with_large(&mut self, step: impl FnOnce(&mut LargeShown), cx: &mut Context<Self>) {
        let row_height = tokens::current(cx).spacing.row_height;
        let file = self.compare.screen.as_mut().and_then(|s| s.file.as_mut());
        let Some((large, control)) =
            file.and_then(|file| Some((file.large.as_mut()?, &file.control)))
        else {
            cx.propagate();
            return;
        };
        large.bar.set_row_height(row_height);
        step(large);
        large.ask(control.as_ref());
        cx.notify();
    }

    /// Binds line, page, start and end scrolling of the large body on the file diff's `root`.
    pub(in crate::window_view::compare_screen) fn large_keys(
        root: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &RowUp, _, cx| {
            view.with_large(|large| large.scroll_by(-1), cx);
        }))
        .on_action(cx.listener(|view, _: &RowDown, _, cx| {
            view.with_large(|large| large.scroll_by(1), cx);
        }))
        .on_action(cx.listener(|view, _: &PageUp, _, cx| {
            view.with_large(|large| large.scroll_by(-large.page()), cx);
        }))
        .on_action(cx.listener(|view, _: &PageDown, _, cx| {
            view.with_large(|large| large.scroll_by(large.page()), cx);
        }))
        .on_action(cx.listener(|view, _: &GoStart, _, cx| {
            view.with_large(|large| large.place(0, 0.), cx);
        }))
        .on_action(cx.listener(|view, _: &GoEnd, _, cx| {
            view.with_large(LargeShown::go_end, cx);
        }))
    }

    /// Follows wheel and scrollbar moves over the large body's `area`. The scrollbar sits inside
    /// it and moves first, so these handlers see the thumb's latest place.
    pub(in crate::window_view::compare_screen) fn large_moves(
        area: Div,
        cx: &mut Context<Self>,
    ) -> Div {
        area.on_scroll(cx.listener(|view, _: &ScrollEvent, _, cx| {
            view.with_large(LargeShown::follow, cx);
        }))
        .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _, cx| {
            event
                .dragging()
                .then(|| view.with_large(LargeShown::follow, cx));
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|view, _: &MouseUpEvent, _, cx| view.with_large(LargeShown::follow, cx)),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|view, _: &MouseUpEvent, _, cx| view.with_large(LargeShown::follow, cx)),
        )
    }

    /// Follows the window's width in the large body's layout (unified below 900 px).
    pub(in crate::window_view) fn large_follow_window(&mut self, window: &Window) {
        let file = self.compare.screen.as_mut().and_then(|s| s.file.as_mut());
        if let Some(file) = file {
            file.auto = super::super::file::layout_for(window);
            file.follow_layout();
        }
    }
}
