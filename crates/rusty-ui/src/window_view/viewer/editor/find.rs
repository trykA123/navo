use std::ops::Range;

use gpui_kit::base::input::RopeExt;
use gpui_kit::component::input::{EditorState, InputEvent};
use gpui_kit::{Focusable, Subscription, Task};
use rusty_core::paging::Needle;

use super::EditorView;
use crate::prelude::*;
use crate::primitives::{TextFieldState, text_field_state};
use crate::window_view::viewer::search::count::{count_after_pause, count_label, count_search};
use crate::window_view::viewer::search::find_all::open_find_all;
use crate::window_view::viewer::search::history::{NeedleKind, Recall};

const TYPE_FIRST: &str = "Type the text to find.";
const NOT_FOUND: &str = "Not found.";

pub(super) struct FindBar {
    pub input: Entity<TextFieldState>,
    pub error: Option<SharedString>,
    _enter: Subscription,
}

#[derive(Default)]
pub(super) struct EditorFind {
    pub bar: Option<FindBar>,
    pub last: Option<String>,
    pub match_case: bool,
    pub hit: Option<u64>,
    counting: Option<Task<()>>,
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

fn search_selection(state: &mut EditorState, cx: &mut Context<EditorState>) {
    let word = word_at_cursor(state).filter(|_| state.selected_range().is_empty());
    if let Some(word) = word {
        state.set_selected_range(word, cx);
    }
    state.open_search(false, cx);
}

impl EditorView {
    pub(super) fn find_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing {
            return self.editor.update(cx, search_selection);
        }
        let text = selection_or_word(self.editor.read(cx));
        self.open_find(window, cx);
        if let Some(bar) = &self.find.bar {
            bar.input
                .update(cx, |input, cx| input.set_value(text, window, cx));
        }
    }

    pub(super) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing {
            return cx.propagate();
        }
        self.go_to = None;
        if let Some(bar) = &self.find.bar {
            let focus = bar.input.read(cx).focus_handle(cx);
            return window.focus(&focus, cx);
        }
        let input = text_field_state("Text to find", window, cx);
        let enter = cx.subscribe_in(
            &input,
            window,
            |view, _, event: &InputEvent, _, cx| match event {
                InputEvent::PressEnter { shift, .. } => view.submit_find(*shift, cx),
                InputEvent::Change => {
                    view.find.counting = Some(count_after_pause(Self::typed_needle, cx));
                }
                _ => {}
            },
        );
        window.focus(&input.read(cx).focus_handle(cx), cx);
        self.find.bar = Some(FindBar {
            input,
            error: None,
            _enter: enter,
        });
        cx.notify();
    }

    pub(super) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.typed(cx) {
            self.find.last = Some(text);
        }
        self.find.bar = None;
        self.find.counting = None;
        let focus = self.editor.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(super) fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.close_go_to(window, cx) {
            return;
        }
        if self.find.bar.is_some() {
            self.close_find(window, cx);
        } else {
            cx.propagate();
        }
    }

    pub(super) fn typed(&self, cx: &App) -> Option<String> {
        self.find
            .bar
            .as_ref()
            .map(|bar| bar.input.read(cx).value().to_string())
            .filter(|text| !text.is_empty())
    }

    fn typed_needle(&self, cx: &App) -> Option<Needle> {
        self.typed(cx).map(|text| self.needle(&text))
    }

    pub(super) fn needle(&self, text: &str) -> Needle {
        Needle::Text {
            text: text.to_owned(),
            match_case: self.find.match_case,
        }
    }

    fn set_find_error(&mut self, error: Option<&str>, cx: &mut Context<Self>) {
        if let Some(bar) = &mut self.find.bar {
            bar.error = error.map(|error| SharedString::from(error.to_owned()));
        }
        cx.notify();
    }

    fn submit_find(&mut self, backward: bool, cx: &mut Context<Self>) {
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

    pub(super) fn find_again(
        &mut self,
        backward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editing {
            return self
                .editor
                .update(cx, |state, cx| find_in_editor(state, backward, cx));
        }
        match self.typed(cx).or_else(|| self.find.last.clone()) {
            Some(text) => self.run_search(text, backward, cx),
            None => self.open_find(window, cx),
        }
    }

    pub(super) fn toggle_case(&mut self, cx: &mut Context<Self>) {
        self.find.match_case = !self.find.match_case;
        cx.notify();
    }

    pub(super) fn recall(&mut self, older: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.find.bar.as_ref().map(|bar| bar.input.clone()) else {
            return;
        };
        let recall = Recall {
            older,
            kind: NeedleKind::Text,
            input: &input,
        };
        let recalled = self.marking.history.recall_into(recall, window, cx);
        if let Some(Some(match_case)) = recalled {
            self.find.match_case = match_case;
        }
        cx.notify();
    }

    pub(super) fn find_all_in_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.typed(cx) else {
            return self.set_find_error(Some(TYPE_FIRST), cx);
        };
        let needle = self.needle(&text);
        self.find.last = Some(text);
        self.marking.remember_search(&needle, cx);
        self.set_find_error(None, cx);
        open_find_all(self, needle, window, cx);
    }

    pub(super) fn count_text(&self) -> Option<SharedString> {
        let counted = self.marking.counted()?;
        Some(count_label(counted, self.find.hit).into())
    }
}
