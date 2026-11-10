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

/// Rows the list holds at once; their offsets (under 2^24 px at 64 px rows) are exact in `f32`.
const WINDOW: u64 = 1 << 18;
/// Rows assumed in view before the first layout measured the list.
const FIRST_VIEW: u64 = 100;
/// A row fraction that absorbs `f32` rounding when an offset turns back into rows.
const SNAP: f32 = 1. / 32.;

impl LargeShown {
    /// The top row: the window's first row plus the whole rows scrolled inside it.
    pub(super) fn top(&self) -> u64 {
        self.base.saturating_add(self.rows_in(self.scrolled()))
    }

    /// Pixels scrolled into the top row.
    pub(super) fn sub(&self) -> f32 {
        let whole = self.rows_in(self.scrolled()) as f32 * f32::from(self.row_height);
        (self.scrolled() - whole).max(0.)
    }

    /// Pixels scrolled inside the list's window, never negative.
    fn scrolled(&self) -> f32 {
        (-f32::from(self.scroll.offset_y())).max(0.)
    }

    /// Whole rows in `pixels`; zero before the row height is known.
    fn rows_in(&self, pixels: f32) -> u64 {
        let height = f32::from(self.row_height);
        let rows = (pixels / height + SNAP).floor();
        if height > 0. && rows.is_finite() && rows > 0. {
            rows as u64
        } else {
            0
        }
    }

    /// Rows the list showed in the last layout, or 100 before the first one.
    pub(super) fn visible(&self) -> u64 {
        match self.rows_in(f32::from(self.scroll.viewport_height())) {
            0 => FIRST_VIEW,
            rows => rows,
        }
    }

    /// The rows the list holds, from `base`.
    pub(super) fn window_len(&self) -> u64 {
        self.map.count().saturating_sub(self.base).min(WINDOW)
    }

    /// Puts row `row` on top with `sub` pixels scrolled into it; the window moves to centre
    /// the row when it nears an edge the file goes past.
    pub(super) fn place(&mut self, row: u64, sub: f32) {
        let count = self.map.count();
        let row = row.min(count.saturating_sub(1));
        let far = row.checked_sub(self.base).is_none_or(|inside| {
            (inside < WINDOW / 4 && self.base > 0)
                || (inside >= WINDOW / 4 * 3 && self.base.saturating_add(WINDOW) < count)
        });
        if far || self.base >= count.max(1) {
            self.base = row
                .saturating_sub(WINDOW / 2)
                .min(count.saturating_sub(WINDOW));
        }
        let inside = row.saturating_sub(self.base) as f32;
        let sub = sub.clamp(0., f32::from(self.row_height));
        self.scroll
            .set_offset_y(px(-(inside * f32::from(self.row_height) + sub)));
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
        large.row_height = row_height;
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

    /// Follows wheel and scrollbar moves over the large body's `area`.
    pub(in crate::window_view::compare_screen) fn large_moves(
        area: Div,
        cx: &mut Context<Self>,
    ) -> Div {
        area.on_scroll(cx.listener(|view, _: &ScrollEvent, _, cx| {
            view.with_large(LargeShown::follow, cx);
        }))
        .capture_any_mouse_up(cx.listener(|view, _: &MouseUpEvent, _, cx| {
            view.with_large(LargeShown::follow, cx);
        }))
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
