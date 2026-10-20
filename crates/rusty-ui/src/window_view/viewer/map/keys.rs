use gpui_kit::{KeyDownEvent, Stateful};

use super::scope::Scope;
use super::select::{Column, Table};
use super::{Drawer, Focus, MapView, facts};
use crate::file_list::typed_char;
use crate::prelude::*;
use crate::window_view::viewer::{
    Back, GoEnd, GoStart, LeftDrawer, OpenGoTo, PageDown, PageUp, RightDrawer, RowDown, RowUp,
};

pub(super) const CONTEXT: &str = "Map";
pub(super) const TABLE_CONTEXT: &str = "MapTable";

gpui_kit::actions!(
    map_view,
    [
        PickItem,
        NextTable,
        SortByName,
        SortBySecond,
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
    crate::commands::bind_namespace(cx, "map_view");
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

impl MapView {
    pub(super) fn move_cursor(&mut self, row: usize, cx: &mut Context<Self>) {
        self.cursor = row.min(self.rows.len().saturating_sub(1));
        self.scroll.scroll_to(self.cursor);
        self.note = None;
        self.refresh_inspection(cx);
        cx.notify();
    }

    fn refresh_inspection(&mut self, cx: &Context<Self>) {
        self.inspection = if self.inspector.read(cx).is_open() {
            self.loaded()
                .zip(self.selected())
                .and_then(|(loaded, at)| facts::inspect(loaded, self.table(), at))
        } else {
            None
        };
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
        self.query.sort_by(column);
        self.run_select(cx);
    }

    fn sort_at(&mut self, key: usize, cx: &mut Context<Self>) {
        if let Some(column) = self.table().sort_keys().get(key) {
            self.sort_by(*column, cx);
        }
    }

    pub(super) fn show_table(&mut self, table: Table, cx: &mut Context<Self>) {
        if self.table() == table || self.loaded().is_none() {
            return;
        }
        self.query.table = table;
        self.rows = std::sync::Arc::new(Vec::new());
        self.go_address = None;
        self.move_cursor(0, cx);
        self.run_select(cx);
    }

    pub(super) fn pick_item(&mut self, at: usize, cx: &mut Context<Self>) {
        let scope = self
            .loaded()
            .zip(self.nav_items.get(at))
            .and_then(|(loaded, item)| item.scope(loaded));
        let Some(scope) = scope else {
            return;
        };
        self.nav_cursor = at;
        let is_all = scope == Scope::All;
        self.query.scope = scope;
        if self.table() == Table::Memory && !is_all {
            return self.show_table(Table::Sections, cx);
        }
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
        self.refresh_inspection(cx);
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
            None if self.query.is_filtering() => self.clear_filter(cx),
            None => cx.propagate(),
        }
    }

    fn clear_filter(&mut self, cx: &mut Context<Self>) {
        self.query.text.clear();
        self.query.scope = Scope::All;
        self.nav_cursor = 0;
        self.run_select(cx);
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

    fn set_text(&mut self, text: String, cx: &mut Context<Self>) {
        self.query.text = text;
        self.run_select(cx);
    }

    fn takes_typing(&self) -> bool {
        self.loaded().is_some() && self.go_to.is_none() && self.focus_in == Focus::Content
    }

    fn type_char(&mut self, typed: char, cx: &mut Context<Self>) {
        if self.takes_typing() {
            let mut text = self.query.text.clone();
            text.push(typed);
            self.set_text(text, cx);
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
        self.set_text(text, cx);
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
            .on_action(cx.listener(|view, _: &NextTable, _, cx| {
                view.show_table(view.table().next(), cx);
            }))
            .on_action(cx.listener(|view, _: &SortByName, _, cx| view.sort_at(0, cx)))
            .on_action(cx.listener(|view, _: &SortBySecond, _, cx| view.sort_at(1, cx)))
            .on_action(cx.listener(|view, _: &SortByAddress, _, cx| view.sort_at(2, cx)))
            .on_action(cx.listener(|view, _: &SortBySize, _, cx| view.sort_at(3, cx)))
    }
}
