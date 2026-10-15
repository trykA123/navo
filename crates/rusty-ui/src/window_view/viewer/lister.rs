use gpui_kit::Stateful;
use rusty_core::{Command, CursorMove};

use super::modes::Pick;
use super::{
    EditFile, FindNext, FindOnEnter, FindPrevious, ListerHex, ListerRich, ListerText, NextFile,
    PreviousFile, ShowHex, ShowRich, ShowText, Slot, Viewer,
};
use crate::prelude::*;
use crate::window_view::FerriteWindow;

const CANNOT_EDIT: &str = "Only UTF-8 text files up to 16 MB can be edited.";

impl Viewer {
    fn is_listing(&self, window: &Window, cx: &App) -> bool {
        !self.is_editing(cx) && self.focus_target(cx).is_focused(window)
    }

    fn can_switch_to_edit(&self) -> bool {
        let Some(editable) = self.editable_kind() else {
            return false;
        };
        let shown = self.body.is_some()
            && self
                .kind
                .as_ref()
                .is_some_and(|kind| kind.id() == editable.id());
        !shown
    }

    pub(super) fn can_start_editing(&self, cx: &App) -> bool {
        let body = self.body.as_ref();
        body.is_some_and(|body| body.can_start_editing(cx)) || self.can_switch_to_edit()
    }
}

impl FerriteWindow {
    fn is_listing(&self, window: &Window, cx: &App) -> bool {
        self.viewer
            .as_ref()
            .is_some_and(|viewer| viewer.is_listing(window, cx))
    }

    fn lister_slot(&mut self, slot: Slot, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_listing(window, cx) {
            cx.propagate();
            return;
        }
        self.show_slot(Pick { slot, edit: false }, window, cx);
    }

    fn edit_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(viewer) = &mut self.viewer else {
            return;
        };
        let started = viewer
            .body
            .as_ref()
            .is_some_and(|body| body.start_editing(cx));
        if started {
            cx.notify();
        } else if viewer.can_switch_to_edit() {
            let pick = Pick {
                slot: Slot::Text,
                edit: true,
            };
            self.show_slot(pick, window, cx);
        } else {
            viewer.say(CANNOT_EDIT, cx);
            cx.notify();
        }
    }

    fn find_next(&mut self, backward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(body) = self.viewer.as_ref().and_then(|viewer| viewer.body.as_ref()) {
            body.find_again(backward, window, cx);
        }
    }

    fn find_on_enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_listing(window, cx) {
            self.find_next(false, window, cx);
        } else {
            cx.propagate();
        }
    }

    fn next_file_row(&self, forward: bool) -> Option<u64> {
        let tab = self.browser.state().active_tab();
        let cursor = tab.view().cursor;
        let listing = tab.listing()?;
        let is_file = |row: &u64| listing.get(*row).is_some_and(|entry| !entry.is_folder);
        if forward {
            (cursor.saturating_add(1)..listing.len()).find(is_file)
        } else {
            (0..cursor.min(listing.len())).rev().find(is_file)
        }
    }

    fn step_file(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_listing(window, cx) {
            cx.propagate();
            return;
        }
        let Some(row) = self.next_file_row(forward) else {
            return;
        };
        let _ = self.browser.command(&Command::PaneCursorMove {
            to: CursorMove::Row(row),
        });
        self.view_row(row, window, cx);
    }

    pub(super) fn slot_actions(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        let show = |slot: Slot| Pick { slot, edit: false };
        root.on_action(cx.listener(move |view, _: &ShowText, window, cx| {
            view.show_slot(show(Slot::Text), window, cx);
        }))
        .on_action(cx.listener(move |view, _: &ShowHex, window, cx| {
            view.show_slot(show(Slot::Hex), window, cx);
        }))
        .on_action(cx.listener(move |view, _: &ShowRich, window, cx| {
            view.show_slot(show(Slot::Rich), window, cx);
        }))
        .on_action(cx.listener(|view, _: &ListerText, window, cx| {
            view.lister_slot(Slot::Text, window, cx);
        }))
        .on_action(cx.listener(|view, _: &ListerHex, window, cx| {
            view.lister_slot(Slot::Hex, window, cx);
        }))
        .on_action(cx.listener(|view, _: &ListerRich, window, cx| {
            view.lister_slot(Slot::Rich, window, cx);
        }))
    }

    pub(super) fn lister_actions(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &EditFile, window, cx| view.edit_file(window, cx)))
            .on_action(cx.listener(|view, _: &FindNext, window, cx| {
                view.find_next(false, window, cx);
            }))
            .on_action(cx.listener(|view, _: &FindPrevious, window, cx| {
                view.find_next(true, window, cx);
            }))
            .on_action(cx.listener(|view, _: &FindOnEnter, window, cx| {
                view.find_on_enter(window, cx);
            }))
            .on_action(cx.listener(|view, _: &NextFile, window, cx| {
                view.step_file(true, window, cx);
            }))
            .on_action(cx.listener(|view, _: &PreviousFile, window, cx| {
                view.step_file(false, window, cx);
            }))
    }
}
