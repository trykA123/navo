//! Search in the editor's find bar: run, step, recall, Find all and the match count.
//! View and edit mode share it; the widget's own search panel stays closed.

use std::ops::Range;

use gpui_kit::base::input::RopeExt;
use gpui_kit::component::input::EditorState;

use super::{NOT_FOUND, TYPE_FIRST};
use crate::prelude::*;
use crate::window_view::viewer::editor::EditorView;
use crate::window_view::viewer::search::count::{count_label, count_search};
use crate::window_view::viewer::search::find_all::open_find_all;
use crate::window_view::viewer::search::history::{NeedleKind, Recall};

fn step_search(
    state: &mut EditorState,
    query: (String, bool),
    backward: bool,
    cx: &mut Context<EditorState>,
) -> Option<std::ops::Range<usize>> {
    let (text, case_insensitive) = query;
    let session = state.search_session();
    let is_new = session.query != text || session.case_insensitive != case_insensitive;
    state.set_search_query(text, case_insensitive, cx);
    if backward {
        return state.previous_search_match(cx);
    }
    if is_new {
        let _ = state.previous_search_match(cx);
    }
    state.next_search_match(cx)
}

fn word_at_cursor(state: &EditorState) -> Option<Range<usize>> {
    let text = state.text();
    let cursor = state.cursor();
    text.word_range(cursor)
        .or_else(|| cursor.checked_sub(1).and_then(|at| text.word_range(at)))
}

fn selection_or_word(state: &EditorState) -> String {
    let selected = state.selected_text().to_string();
    if !selected.is_empty() {
        return selected.lines().next().unwrap_or_default().to_owned();
    }
    word_at_cursor(state)
        .map(|range| state.text().slice(range).to_string())
        .unwrap_or_default()
}

impl EditorView {
    pub(in super::super) fn find_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = selection_or_word(self.editor.read(cx));
        self.open_find(window, cx);
        if let Some(bar) = &self.find.bar {
            bar.input
                .update(cx, |input, cx| input.set_value(text, window, cx));
        }
    }

    pub(super) fn submit_find(&mut self, backward: bool, cx: &mut Context<Self>) {
        match self.typed(cx) {
            Some(text) => self.run_search(text, backward, cx),
            None => self.set_find_error(Some(TYPE_FIRST), cx),
        }
    }

    fn run_search(&mut self, text: String, backward: bool, cx: &mut Context<Self>) {
        let needle = self.needle(&text);
        self.find.last = Some(text.clone());
        self.find.counting = None;
        count_search(self, needle.clone(), cx);
        self.marking.remember_search(&needle, cx);
        let case_insensitive = !self.find.match_case;
        let hit = self.editor.update(cx, |state, cx| {
            step_search(state, (text, case_insensitive), backward, cx)
        });
        self.find.hit = hit.as_ref().map(|range| range.start as u64);
        self.set_find_error(hit.is_none().then_some(NOT_FOUND), cx);
    }

    pub(in super::super) fn find_again(
        &mut self,
        backward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.typed(cx).or_else(|| self.find.last.clone()) {
            Some(text) => self.run_search(text, backward, cx),
            None => self.open_find(window, cx),
        }
    }

    pub(in super::super) fn toggle_case(&mut self, cx: &mut Context<Self>) {
        self.find.match_case = !self.find.match_case;
        cx.notify();
    }

    pub(in super::super) fn recall(
        &mut self,
        older: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.find.bar.as_ref().map(|bar| bar.input.clone()) else {
            return;
        };
        let recall = Recall {
            older,
            kind: NeedleKind::Text,
            input: &input,
        };
        let recalled = self.marking.history.recall_into(&recall, window, cx);
        if let Some(Some(match_case)) = recalled {
            self.find.match_case = match_case;
        }
        cx.notify();
    }

    pub(in super::super) fn find_all_in_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = self.typed(cx) else {
            return self.set_find_error(Some(TYPE_FIRST), cx);
        };
        let needle = self.needle(&text);
        self.find.last = Some(text);
        self.marking.remember_search(&needle, cx);
        self.set_find_error(None, cx);
        open_find_all(self, needle, window, cx);
    }

    pub(in super::super) fn count_text(&self) -> Option<SharedString> {
        let counted = self.marking.counted()?;
        Some(count_label(counted, self.find.hit).into())
    }
}
