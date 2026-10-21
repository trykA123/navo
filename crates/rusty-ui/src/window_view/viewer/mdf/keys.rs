use gpui_kit::Stateful;

use super::MdfView;
use crate::prelude::*;
use crate::window_view::viewer::{
    Back, GoEnd, GoStart, PageDown, PageUp, RightDrawer, RowDown, RowUp,
};

pub(super) const CONTEXT: &str = "Mdf";

fn step(at: usize, by: i64, len: usize) -> usize {
    let moved = i64::try_from(at)
        .unwrap_or(i64::MAX)
        .saturating_add(by)
        .max(0);
    usize::try_from(moved)
        .unwrap_or(usize::MAX)
        .min(len.saturating_sub(1))
}

impl MdfView {
    pub(super) fn move_cursor(&mut self, row: usize, cx: &mut Context<Self>) {
        self.cursor = row.min(self.row_count().saturating_sub(1));
        self.scroll.scroll_to(self.cursor);
        self.note = None;
        cx.notify();
    }

    fn move_by(&mut self, by: i64, cx: &mut Context<Self>) {
        self.move_cursor(step(self.cursor, by, self.row_count()), cx);
    }

    fn page_by(&self, backward: bool) -> i64 {
        let page = i64::try_from(self.page.max(1)).unwrap_or(1);
        if backward { -page } else { page }
    }

    pub(super) fn open_inspector_if_asked(&mut self, cx: &mut Context<Self>) {
        if self.opens_inspector && !self.inspector.read(cx).is_open() {
            self.toggle_inspector(cx);
        }
    }

    pub(super) fn toggle_inspector(&mut self, cx: &mut Context<Self>) {
        self.inspector.update(cx, |sheet, cx| {
            if sheet.is_open() {
                sheet.close(cx);
            } else {
                sheet.open(cx);
            }
        });
        cx.notify();
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        if self.inspector.read(cx).is_open() {
            self.toggle_inspector(cx);
        } else {
            cx.propagate();
        }
    }

    pub(super) fn keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &RowUp, _, cx| view.move_by(-1, cx)))
            .on_action(cx.listener(|view, _: &RowDown, _, cx| view.move_by(1, cx)))
            .on_action(cx.listener(|view, _: &PageUp, _, cx| {
                view.move_by(view.page_by(true), cx);
            }))
            .on_action(cx.listener(|view, _: &PageDown, _, cx| {
                view.move_by(view.page_by(false), cx);
            }))
            .on_action(cx.listener(|view, _: &GoStart, _, cx| view.move_by(i64::MIN, cx)))
            .on_action(cx.listener(|view, _: &GoEnd, _, cx| view.move_by(i64::MAX, cx)))
            .on_action(cx.listener(|view, _: &Back, _, cx| view.back(cx)))
            .on_action(cx.listener(|view, _: &RightDrawer, _, cx| view.toggle_inspector(cx)))
    }
}
