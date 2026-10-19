use gpui_kit::component::input::InputEvent;
use gpui_kit::{Focusable, Stateful};

use super::text::{address, parse_address, span, whole};
use super::{Drawer, Focus, GoTo, MemoryView};
use crate::prelude::*;
use crate::primitives::text_field_state;
use crate::window_view::viewer::{
    Back, GoEnd, GoStart, OpenGoTo, PageDown, PageUp, RowDown, RowUp,
};

pub(super) const CONTEXT: &str = "MemoryImage";

gpui_kit::actions!(memory_image, [ToggleNavigator, ToggleInspector, PickItem]);

pub(in crate::window_view::viewer) fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "memory_image");
}

fn step(at: u64, by: i64, len: u64) -> u64 {
    let last = len.saturating_sub(1);
    if by < 0 {
        at.saturating_sub(by.unsigned_abs())
    } else {
        at.saturating_add(by.unsigned_abs()).min(last)
    }
}

impl MemoryView {
    pub(super) fn move_cursor(&mut self, row: u64, cx: &mut Context<Self>) {
        self.cursor = row.min(self.rows().saturating_sub(1));
        self.scroll
            .scroll_to(usize::try_from(self.cursor).unwrap_or(usize::MAX));
        if let Some(item) = self.item_of_row(self.cursor) {
            self.selected = item;
        }
        self.note = None;
        cx.notify();
    }

    fn move_by(&mut self, by: i64, cx: &mut Context<Self>) {
        match self.focus_in {
            Focus::Content => self.move_cursor(step(self.cursor, by, self.rows()), cx),
            Focus::Navigator => {
                let items = self.ready().map_or(0, |ready| ready.items.len()) as u64;
                let at = step(self.selected as u64, by, items);
                self.selected = usize::try_from(at).unwrap_or(0);
                self.nav_scroll.scroll_to(self.selected);
                cx.notify();
            }
        }
    }

    fn page_by(&self, backward: bool) -> i64 {
        let page = i64::try_from(self.page.max(1)).unwrap_or(1);
        if backward { -page } else { page }
    }

    pub(super) fn pick_item(&mut self, item: usize, cx: &mut Context<Self>) {
        let row = self.ready().and_then(|ready| {
            let start = ready.items.get(item)?.span().start;
            ready.layout.row_of(start)
        });
        let Some(row) = row else {
            return;
        };
        self.move_cursor(row, cx);
        self.selected = item;
        self.scroll
            .scroll_to_top(usize::try_from(row).unwrap_or(usize::MAX));
        let opens = cx
            .try_global::<super::AutomotiveLook>()
            .is_some_and(|look| look.opens_inspector);
        if opens && !self.inspector.read(cx).is_open() {
            self.toggle_inspector(cx);
        }
    }

    pub(super) fn toggle_navigator(&mut self, cx: &mut Context<Self>) {
        if self.navigator.read(cx).is_open() {
            self.close_navigator(cx);
            return;
        }
        if let Some(item) = self.item_of_row(self.cursor) {
            self.selected = item;
        }
        self.focus_in = Focus::Navigator;
        self.top_drawer = Drawer::Navigator;
        self.nav_scroll.scroll_to(self.selected);
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

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.go_to.take().is_some() {
            window.focus(&self.focus, cx);
            cx.notify();
        } else if let Some(drawer) = self.top_open(cx) {
            match drawer {
                Drawer::Navigator => self.close_navigator(cx),
                Drawer::Inspector => self.toggle_inspector(cx),
            }
        } else {
            cx.propagate();
        }
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

    fn open_go_to(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ready().is_none() {
            return;
        }
        let input = text_field_state("Hex address, like 8000400", window, cx);
        let enter = cx.subscribe_in(&input, window, |view, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                view.submit_go_to(window, cx);
            }
        });
        window.focus(&input.read(cx).focus_handle(cx), cx);
        self.go_to = Some(GoTo {
            input,
            error: None,
            _enter: enter,
        });
        cx.notify();
    }

    fn submit_go_to(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(go_to) = &self.go_to else {
            return;
        };
        let typed = go_to.input.read(cx).value().to_string();
        match self.row_for(&typed) {
            Ok(row) => {
                self.go_to = None;
                window.focus(&self.focus, cx);
                self.move_cursor(row, cx);
            }
            Err(error) => {
                if let Some(go_to) = &mut self.go_to {
                    go_to.error = Some(error.into());
                }
                cx.notify();
            }
        }
    }

    fn row_for(&self, typed: &str) -> Result<u64, String> {
        let at = parse_address(typed)?;
        let ready = self.ready().ok_or("The image is still being read.")?;
        ready.layout.row_of(at).ok_or_else(|| {
            let whole = whole(&ready.image);
            format!(
                "{} is outside the image ({})",
                address(at, ready.wide),
                span(&whole, ready.wide)
            )
        })
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
            .on_action(cx.listener(|view, _: &OpenGoTo, window, cx| view.open_go_to(window, cx)))
            .on_action(cx.listener(|view, _: &Back, window, cx| view.back(window, cx)))
            .on_action(cx.listener(|view, _: &ToggleNavigator, _, cx| view.toggle_navigator(cx)))
            .on_action(cx.listener(|view, _: &ToggleInspector, _, cx| view.toggle_inspector(cx)))
            .on_action(cx.listener(|view, _: &PickItem, _, cx| {
                if view.focus_in == Focus::Navigator {
                    view.pick_item(view.selected, cx);
                } else {
                    cx.propagate();
                }
            }))
    }
}
