use gpui_kit::{KeyDownEvent, Stateful};

use super::{Drawer, Focus, TraceView};
use crate::file_list::typed_char;
use crate::prelude::*;
use crate::window_view::viewer::{Back, GoEnd, GoStart, PageDown, PageUp, RowDown, RowUp};

pub(super) const CONTEXT: &str = "CanTrace";

gpui_kit::actions!(
    can_trace,
    [
        ToggleNavigator,
        ToggleInspector,
        PickId,
        SortIds,
        TypeOne,
        TypeThree,
        TypeFive,
        FilterBack,
    ]
);

pub(in crate::window_view::viewer) fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "can_trace");
}

fn step(at: u64, by: i64, len: u64) -> u64 {
    let last = len.saturating_sub(1);
    if by < 0 {
        at.saturating_sub(by.unsigned_abs())
    } else {
        at.saturating_add(by.unsigned_abs()).min(last)
    }
}

impl TraceView {
    pub(super) fn move_cursor(&mut self, shown: u64, cx: &mut Context<Self>) {
        self.cursor = shown.min(self.shown_rows().saturating_sub(1));
        self.scroll
            .scroll_to(usize::try_from(self.cursor).unwrap_or(usize::MAX));
        self.note = None;
        cx.notify();
    }

    fn move_by(&mut self, by: i64, cx: &mut Context<Self>) {
        match self.focus_in {
            Focus::Content => {
                let len = self.shown_rows();
                let to = step(self.cursor, by, len);
                self.follow_moved(to + 1 >= len);
                self.move_cursor(to, cx);
            }
            Focus::Navigator => {
                let len = self.nav.items.len() as u64;
                let at = step(self.nav.selected as u64, by, len);
                self.nav.selected = usize::try_from(at).unwrap_or(0);
                self.nav_scroll.scroll_to(self.nav.selected);
                cx.notify();
            }
        }
    }

    fn page_by(&self, backward: bool) -> i64 {
        let page = i64::try_from(self.page.max(1)).unwrap_or(1);
        if backward { -page } else { page }
    }

    pub(super) fn toggle_navigator(&mut self, cx: &mut Context<Self>) {
        if self.navigator.read(cx).is_open() {
            self.close_navigator(cx);
            return;
        }
        self.refresh_nav();
        if let Some(filter) = &self.filter {
            let key = Some(filter.key);
            self.nav.selected = self
                .nav
                .items
                .iter()
                .position(|item| *item == key)
                .unwrap_or(0);
        }
        self.focus_in = Focus::Navigator;
        self.top_drawer = Drawer::Navigator;
        self.nav_scroll.scroll_to(self.nav.selected);
        self.navigator.update(cx, |sheet, cx| sheet.open(cx));
        cx.notify();
    }

    pub(super) fn close_navigator(&mut self, cx: &mut Context<Self>) {
        self.focus_in = Focus::Content;
        self.nav.typed.clear();
        self.navigator.update(cx, |sheet, cx| sheet.close(cx));
        cx.notify();
    }

    pub(super) fn toggle_inspector(&mut self, cx: &mut Context<Self>) {
        let is_open = self.inspector.read(cx).is_open();
        if !is_open {
            self.top_drawer = Drawer::Inspector;
        }
        self.inspector.update(cx, |sheet, cx| {
            if is_open {
                sheet.close(cx);
            } else {
                sheet.open(cx);
            }
        });
        cx.notify();
    }

    fn top_open(&self, cx: &App) -> Option<Drawer> {
        let navigator = self.navigator.read(cx).is_open();
        let inspector = self.inspector.read(cx).is_open();
        match (navigator, inspector) {
            (true, true) => Some(self.top_drawer),
            (true, false) => Some(Drawer::Navigator),
            (false, true) => Some(Drawer::Inspector),
            (false, false) => None,
        }
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        match self.top_open(cx) {
            Some(Drawer::Navigator) => self.close_navigator(cx),
            Some(Drawer::Inspector) => self.toggle_inspector(cx),
            None if self.filter.is_some() => self.set_filter(None, cx),
            None => cx.propagate(),
        }
    }

    fn type_or_pass(&mut self, typed: char, cx: &mut Context<Self>) {
        if self.focus_in == Focus::Navigator {
            self.type_char(typed, cx);
        } else {
            cx.propagate();
        }
    }

    fn type_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if self.focus_in != Focus::Navigator {
            return;
        }
        if let Some(typed) = typed_char(&event.keystroke) {
            self.type_char(typed, cx);
            cx.stop_propagation();
        }
    }

    fn in_navigator(
        &mut self,
        cx: &mut Context<Self>,
        act: impl FnOnce(&mut Self, &mut Context<Self>),
    ) {
        if self.focus_in == Focus::Navigator {
            act(self, cx);
        } else {
            cx.propagate();
        }
    }

    pub(super) fn moves(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
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
    }

    pub(super) fn commands(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &Back, _, cx| view.back(cx)))
            .on_action(cx.listener(|view, _: &ToggleNavigator, _, cx| view.toggle_navigator(cx)))
            .on_action(cx.listener(|view, _: &ToggleInspector, _, cx| view.toggle_inspector(cx)))
            .on_action(cx.listener(|view, _: &PickId, _, cx| {
                view.in_navigator(cx, |view, cx| view.pick_nav(view.nav.selected, cx));
            }))
            .on_action(cx.listener(|view, _: &SortIds, _, cx| {
                view.in_navigator(cx, Self::toggle_sort);
            }))
            .on_action(cx.listener(|view, _: &FilterBack, _, cx| {
                view.in_navigator(cx, Self::erase_char);
            }))
            .on_action(cx.listener(|view, _: &TypeOne, _, cx| view.type_or_pass('1', cx)))
            .on_action(cx.listener(|view, _: &TypeThree, _, cx| view.type_or_pass('3', cx)))
            .on_action(cx.listener(|view, _: &TypeFive, _, cx| view.type_or_pass('5', cx)))
            .on_key_down(cx.listener(|view, event: &KeyDownEvent, _, cx| {
                view.type_key(event, cx);
            }))
    }
}
