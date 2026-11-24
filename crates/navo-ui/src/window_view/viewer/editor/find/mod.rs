//! The editor's find bar: its state, opening and closing it, and the typed needle.
//! Searching and stepping live in `bar.rs`, regex mode in `regex.rs`, and the edit-mode
//! replace field in `replace.rs`.

mod bar;
mod regex;
mod replace;

use gpui_kit::component::input::InputEvent;
use gpui_kit::{Focusable, Subscription, Task};
use navo_core::paging::Needle;

use super::EditorView;
use crate::prelude::*;
use crate::primitives::{TextFieldState, text_field_state};
use crate::window_view::viewer::search::count::count_after_pause;

const TYPE_FIRST: &str = "Type the text to find.";
const NOT_FOUND: &str = "Not found.";

pub(super) struct FindBar {
    pub input: Entity<TextFieldState>,
    pub error: Option<SharedString>,
    pub replace: replace::ReplaceField,
    _enter: Subscription,
}

#[derive(Default)]
pub(super) struct EditorFind {
    pub bar: Option<FindBar>,
    pub last: Option<String>,
    pub match_case: bool,
    /// Regex mode; off by default and kept only while this view lives.
    pub regex: bool,
    /// The regex-mode match count over the live text; plain mode counts through `marking`.
    pub regex_count: regex::RegexCount,
    pub hit: Option<u64>,
    counting: Option<Task<()>>,
}

impl EditorView {
    pub(super) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                InputEvent::Change if view.find.regex => {
                    view.find.counting = Some(regex::recount_after_pause(cx));
                }
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
            replace: replace::replace_field(window, cx),
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

    /// The typed text as a plain needle, or `None` in regex mode or when nothing is typed.
    fn typed_needle(&self, cx: &App) -> Option<Needle> {
        self.typed(cx)
            .filter(|_| !self.find.regex)
            .map(|text| self.needle(&text))
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
}
