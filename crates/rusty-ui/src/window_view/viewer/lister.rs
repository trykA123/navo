use gpui_kit::Stateful;
use gpui_kit::component::input::EditorState;
use rusty_core::{Command, CursorMove};

use super::modes::Pick;
use super::{
    EditFile, FindNext, FindPrevious, ListerHex, ListerRich, ListerText, NextFile, PreviousFile,
    Shape, ShowHex, ShowRich, ShowText, Slot, Viewer,
};
use crate::prelude::*;
use crate::window_view::FerriteWindow;

const TOO_BIG_TO_EDIT: &str = "Only text files up to 16 MB can be edited.";

impl Viewer {
    fn is_listing(&self, window: &Window, cx: &App) -> bool {
        !self.editing && self.focus_target(cx).is_focused(window)
    }

    pub(super) fn can_start_editing(&self) -> bool {
        if self.editing {
            return false;
        }
        match &self.editor {
            Some(_) => self.can_edit,
            None => self
                .kind_in(Slot::Text)
                .is_some_and(|kind| kind.shape(&self.sniff()) == Shape::Editor),
        }
    }
}

fn find_in_editor(state: &mut EditorState, backward: bool, cx: &mut Context<EditorState>) {
    let session = state.search_session();
    if session.query.is_empty() {
        state.open_search(false, cx);
        return;
    }
    let (query, case_insensitive) = (session.query.clone(), session.case_insensitive);
    state.set_search_query(query, case_insensitive, cx);
    let _ = if backward {
        state.previous_search_match(cx)
    } else {
        state.next_search_match(cx)
    };
}

impl FerriteWindow {
    fn lister_slot(&mut self, slot: Slot, window: &mut Window, cx: &mut Context<Self>) {
        let listing = self
            .viewer
            .as_ref()
            .is_some_and(|viewer| viewer.is_listing(window, cx));
        if !listing {
            cx.propagate();
            return;
        }
        self.show_slot(Pick { slot, edit: false }, window, cx);
    }

    fn edit_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(viewer) = &self.viewer else {
            return;
        };
        if viewer.editor.is_some() {
            self.start_editing(cx);
        } else if viewer.can_start_editing() {
            let pick = Pick {
                slot: Slot::Text,
                edit: true,
            };
            self.show_slot(pick, window, cx);
        } else if let Some(paged) = viewer.paged.clone() {
            paged.update(cx, |paged, cx| paged.set_note(TOO_BIG_TO_EDIT, cx));
        }
    }

    fn find_next(&mut self, backward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(viewer) = &self.viewer else {
            return;
        };
        if let Some(paged) = viewer.paged.clone() {
            paged.update(cx, |paged, cx| paged.find_again(backward, window, cx));
        } else if let Some(editor) = viewer.editor.clone() {
            editor.update(cx, |state, cx| find_in_editor(state, backward, cx));
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
        let listing = self
            .viewer
            .as_ref()
            .is_some_and(|viewer| viewer.is_listing(window, cx));
        if !listing {
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
            .on_action(cx.listener(|view, _: &NextFile, window, cx| {
                view.step_file(true, window, cx);
            }))
            .on_action(cx.listener(|view, _: &PreviousFile, window, cx| {
                view.step_file(false, window, cx);
            }))
    }
}
