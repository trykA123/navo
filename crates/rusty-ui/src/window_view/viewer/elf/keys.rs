use gpui_kit::{KeyDownEvent, Stateful};
use rusty_automotive::{Column, Order};

use super::{Drawer, ElfView, Focus};
use crate::file_list::typed_char;
use crate::prelude::*;
use crate::window_view::viewer::{
    Back, GoEnd, GoStart, LeftDrawer, OpenGoTo, PageDown, PageUp, RightDrawer, RowDown, RowUp,
};

pub(super) const CONTEXT: &str = "Elf";
pub(super) const TABLE_CONTEXT: &str = "ElfTable";

gpui_kit::actions!(
    elf_view,
    [
        PickItem,
        SortByName,
        SortBySection,
        SortByAddress,
        SortBySize,
        TypeN,
        TypeP,
        TypeOne,
        TypeThree,
        TypeFive
    ]
);

pub(in crate::window_view::viewer) fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "elf_view");
}

fn step(at: usize, by: i64, len: usize) -> usize {
    let moved = i64::try_from(at)
        .unwrap_or(i64::MAX)
        .saturating_add(by)
        .max(0);
    usize::try_from(moved)
        .unwrap_or(usize::MAX)
        .min(len.saturating_sub(1))
}

impl ElfView {
    pub(super) fn move_cursor(&mut self, row: usize, cx: &mut Context<Self>) {
        self.cursor = row.min(self.rows.len().saturating_sub(1));
        self.scroll.scroll_to(self.cursor);
        self.note = None;
        cx.notify();
    }

    fn move_by(&mut self, by: i64, cx: &mut Context<Self>) {
        self.go_address = None;
        match self.focus_in {
            Focus::Content => self.move_cursor(step(self.cursor, by, self.rows.len()), cx),
            Focus::Navigator => {
                self.nav_cursor = step(self.nav_cursor, by, self.nav_items.len());
                self.nav_scroll.scroll_to(self.nav_cursor);
                cx.notify();
            }
        }
    }

    fn page_by(&self, backward: bool) -> i64 {
        let page = i64::try_from(self.page.max(1)).unwrap_or(1);
        if backward { -page } else { page }
    }

    pub(super) fn sort_by(&mut self, column: Column, cx: &mut Context<Self>) {
        let old = self.query.order;
        self.query.order = Order {
            column,
            descending: old.column == column && !old.descending,
        };
        self.run_select(cx);
    }

    pub(super) fn pick_item(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(item) = self.nav_items.get(at) else {
            return;
        };
        self.nav_cursor = at;
        self.query.scope = item.scope();
        self.run_select(cx);
    }

    pub(super) fn open_inspector_if_asked(&mut self, cx: &mut Context<Self>) {
        if self.opens_inspector && !self.inspector.read(cx).is_open() {
            self.toggle_inspector(cx);
        }
    }

    pub(super) fn toggle_navigator(&mut self, cx: &mut Context<Self>) {
        if self.navigator.read(cx).is_open() {
            self.close_navigator(cx);
            return;
        }
        self.focus_in = Focus::Navigator;
        self.top_drawer = Drawer::Navigator;
        self.nav_scroll.scroll_to(self.nav_cursor);
        self.navigator.update(cx, |sheet, cx| sheet.open(cx));
        cx.notify();
    }

    pub(super) fn close_navigator(&mut self, cx: &mut Context<Self>) {
        self.focus_in = Focus::Content;
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

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.close_go_to(window, cx) {
            return;
        }
        match self.top_open(cx) {
            Some(Drawer::Navigator) => self.close_navigator(cx),
            Some(Drawer::Inspector) => self.toggle_inspector(cx),
            None if self.is_filtering() => self.set_filter(String::new(), cx),
            None => cx.propagate(),
        }
    }

    fn enter(&mut self, cx: &mut Context<Self>) {
        if self.focus_in == Focus::Navigator {
            self.pick_item(self.nav_cursor, cx);
        } else if !self.inspector.read(cx).is_open() && self.selected().is_some() {
            self.toggle_inspector(cx);
        } else {
            cx.propagate();
        }
    }

    fn set_filter(&mut self, text: String, cx: &mut Context<Self>) {
        self.query.text = text;
        self.run_select(cx);
    }

    fn takes_typing(&self) -> bool {
        self.elf().is_some() && self.go_to.is_none() && self.focus_in == Focus::Content
    }

    fn type_char(&mut self, typed: char, cx: &mut Context<Self>) {
        if self.takes_typing() {
            let mut text = self.query.text.clone();
            text.push(typed);
            self.set_filter(text, cx);
        }
    }

    fn type_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if !self.takes_typing() {
            return;
        }
        let mut text = self.query.text.clone();
        if let Some(typed) = typed_char(&event.keystroke) {
            text.push(typed);
        } else if event.keystroke.key == "backspace" && !text.is_empty() {
            text.pop();
        } else {
            return;
        }
        cx.stop_propagation();
        self.set_filter(text, cx);
    }

    pub(super) fn typing(table: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        table
            .on_action(cx.listener(|view, _: &TypeN, _, cx| view.type_char('n', cx)))
            .on_action(cx.listener(|view, _: &TypeP, _, cx| view.type_char('p', cx)))
            .on_action(cx.listener(|view, _: &TypeOne, _, cx| view.type_char('1', cx)))
            .on_action(cx.listener(|view, _: &TypeThree, _, cx| view.type_char('3', cx)))
            .on_action(cx.listener(|view, _: &TypeFive, _, cx| view.type_char('5', cx)))
            .on_key_down(cx.listener(|view, event, _, cx| view.type_key(event, cx)))
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
            .on_action(cx.listener(|view, _: &Back, window, cx| view.back(window, cx)))
            .on_action(cx.listener(|view, _: &OpenGoTo, window, cx| view.open_go_to(window, cx)))
            .on_action(cx.listener(|view, _: &LeftDrawer, _, cx| view.toggle_navigator(cx)))
            .on_action(cx.listener(|view, _: &RightDrawer, _, cx| view.toggle_inspector(cx)))
            .on_action(cx.listener(|view, _: &PickItem, _, cx| view.enter(cx)))
            .on_action(cx.listener(|view, _: &SortByName, _, cx| view.sort_by(Column::Name, cx)))
            .on_action(cx.listener(|view, _: &SortBySection, _, cx| {
                view.sort_by(Column::Section, cx);
            }))
            .on_action(cx.listener(|view, _: &SortByAddress, _, cx| {
                view.sort_by(Column::Address, cx);
            }))
            .on_action(cx.listener(|view, _: &SortBySize, _, cx| view.sort_by(Column::Size, cx)))
    }
}
