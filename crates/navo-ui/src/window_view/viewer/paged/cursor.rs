//! The hex cursor: arrows, page keys and a click move it a byte at a time, and the view
//! scrolls to keep it on screen. Text mode keeps the plain scroll keys.

use std::rc::Rc;

use navo_core::paging::HEX_WIDTH;

use super::PagedView;
use super::lines::PagedKind;
use super::moves::Move;
use super::row_view::HexCursor;
use crate::prelude::*;

impl PagedView {
    /// The last byte the cursor may sit on (0 for an empty file).
    fn last_byte(&self) -> u64 {
        self.source.len().saturating_sub(1)
    }

    /// Keeps the cursor inside the file after it shrank.
    pub(super) fn clamp_cursor(&mut self) {
        self.cursor = self.cursor.min(self.last_byte());
    }

    /// Moves the cursor by `delta` bytes, clamped to the file, and scrolls it into view.
    fn move_cursor(&mut self, delta: i64, cx: &mut Context<Self>) {
        let moved = i128::from(self.cursor) + i128::from(delta);
        let clamped = moved.clamp(0, i128::from(self.last_byte()));
        self.cursor = u64::try_from(clamped).unwrap_or_default();
        self.reveal_cursor(cx);
        cx.notify();
    }

    /// Scrolls the least that brings the cursor's row on screen.
    fn reveal_cursor(&mut self, cx: &mut Context<Self>) {
        let start = self.cursor - self.cursor % HEX_WIDTH;
        let shown = self.rows.max(1) * HEX_WIDTH;
        if self.cursor < self.top {
            self.user_move(Move::Offset(start), cx);
        } else if self.cursor >= self.top + shown {
            self.user_move(Move::Offset((start + HEX_WIDTH).saturating_sub(shown)), cx);
        }
    }

    /// Up and Down: the cursor moves a row in hex mode; text mode scrolls `rows` rows.
    pub(super) fn row_key(&mut self, rows: i64, cx: &mut Context<Self>) {
        if self.kind == PagedKind::Hex {
            self.move_cursor(rows.saturating_mul(HEX_WIDTH as i64), cx);
        } else {
            self.scroll_rows(rows, cx);
        }
    }

    /// Left and Right: the cursor moves a byte in hex mode; text mode shifts the columns.
    pub(super) fn column_key(&mut self, right: bool, cx: &mut Context<Self>) {
        if self.kind == PagedKind::Hex {
            self.move_cursor(if right { 1 } else { -1 }, cx);
        } else {
            self.shift_by(right, cx);
        }
    }

    /// Page Up and Page Down scroll a screen; the hex cursor moves with it.
    pub(super) fn page_key(&mut self, down: bool, cx: &mut Context<Self>) {
        let page = if down { self.page() } else { -self.page() };
        if self.kind == PagedKind::Hex {
            let moved = i128::from(self.cursor) + i128::from(page) * i128::from(HEX_WIDTH);
            let clamped = moved.clamp(0, i128::from(self.last_byte()));
            self.cursor = u64::try_from(clamped).unwrap_or_default();
        }
        self.scroll_rows(page, cx);
    }

    /// Ctrl+Home and Ctrl+End go to the file's ends; the hex cursor goes to its first or last byte.
    pub(super) fn ends_key(&mut self, end: bool, cx: &mut Context<Self>) {
        if self.kind == PagedKind::Hex {
            self.cursor = if end { self.last_byte() } else { 0 };
        }
        self.user_move(if end { Move::End } else { Move::Offset(0) }, cx);
    }

    /// Puts the cursor on the clicked byte and gives the view the keyboard focus.
    fn pick_byte(&mut self, at: u64, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        self.cursor = at.min(self.last_byte());
        cx.notify();
    }

    /// The cursor the hex rows draw, and the click that moves it.
    pub(super) fn hex_cursor(&self, cx: &mut Context<Self>) -> HexCursor {
        let view = cx.weak_entity();
        HexCursor {
            at: Some(self.cursor),
            pick: Some(Rc::new(move |at, window, cx| {
                let _ = view.update(cx, |view, cx| view.pick_byte(at, window, cx));
            })),
        }
    }
}
