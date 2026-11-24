//! Replace in the editor's find bar, shown in edit mode only: Replace changes the current
//! match and steps on; Replace all rewrites every match as one undo step. Plain mode is
//! literal; regex mode expands `$1`, `${name}` and `$$`.

use std::ops::Range;

use gpui_kit::component::input::InputEvent;
use gpui_kit::{Focusable, Subscription};
use navo_l10n::tr;

use super::{NOT_FOUND, TYPE_FIRST};
use crate::prelude::*;
use crate::primitives::{Button, TextField, TextFieldState, text_field_state};
use crate::tokens::Theme;
use crate::window_view::viewer::editor::EditorView;

/// The least width, in pixels, the replace field shrinks to in a narrow header.
const MIN_INPUT: f32 = 48.;

/// The replace field next to the find field; Enter in it runs Replace.
pub(in super::super) struct ReplaceField {
    pub input: Entity<TextFieldState>,
    _enter: Subscription,
}

/// Makes the replace field and wires its Enter key to Replace.
pub(super) fn replace_field(window: &mut Window, cx: &mut Context<EditorView>) -> ReplaceField {
    let input = text_field_state(tr("editor-replace-placeholder"), window, cx);
    let enter = cx.subscribe_in(&input, window, |view, _, event: &InputEvent, window, cx| {
        if let InputEvent::PressEnter { .. } = event {
            view.replace_one(window, cx);
        }
    });
    ReplaceField {
        input,
        _enter: enter,
    }
}

/// Copies `text` with each byte range in `ranges` (sorted, not overlapping) swapped for
/// `with`, taken literally: a `$` stays a `$`.
fn swapped(text: &str, ranges: &[Range<usize>], with: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for range in ranges {
        out.push_str(&text[at..range.start]);
        out.push_str(with);
        at = range.end;
    }
    out.push_str(&text[at..]);
    out
}

impl EditorView {
    /// Opens the find bar; in edit mode it then focuses the replace field.
    pub(in super::super) fn open_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_find(window, cx);
        if let Some(bar) = self.find.bar.as_ref().filter(|_| self.editing) {
            let focus = bar.replace.input.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
    }

    /// The typed find text and the replacement, or `None` (with an error shown) when the
    /// bar is closed, the view is read-only or the find field is empty.
    fn replace_inputs(&mut self, cx: &mut Context<Self>) -> Option<(String, String)> {
        let bar = self.find.bar.as_ref().filter(|_| self.editing)?;
        let with = bar.replace.input.read(cx).value().to_string();
        let Some(text) = self.typed(cx) else {
            self.set_find_error(Some(TYPE_FIRST), cx);
            return None;
        };
        Some((text, with))
    }

    /// Replaces the current match with the replacement, taken literally, and steps to the next.
    pub(super) fn replace_one(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((text, with)) = self.replace_inputs(cx) else {
            return;
        };
        if self.find.regex {
            return self.regex_replace_one((&text, &with), window, cx);
        }
        let case_insensitive = !self.find.match_case;
        let done = self.editor.update(cx, |state, cx| {
            state.set_search_query(text, case_insensitive, cx);
            state.replace_current_search_match(&with, window, cx)
        });
        self.set_find_error((!done).then_some(NOT_FOUND), cx);
    }

    /// Replaces every match as one undo step; outside the edit-mode bar the key passes on.
    pub(in super::super) fn replace_all_matches(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.editing || self.find.bar.is_none() {
            return cx.propagate();
        }
        let Some((text, with)) = self.replace_inputs(cx) else {
            return;
        };
        if self.find.regex {
            return self.regex_replace_all((&text, &with), window, cx);
        }
        let case_insensitive = !self.find.match_case;
        let count = self.editor.update(cx, |state, cx| {
            state.set_search_query(text, case_insensitive, cx);
            let ranges = state.search_session().matcher.matched_ranges();
            if !ranges.is_empty() {
                // One whole-text `replace_all` is one undo entry.
                let next = swapped(&state.value(), &ranges, &with);
                state.replace_all(next, window, cx);
            }
            ranges.len()
        });
        self.set_find_error((count == 0).then_some(NOT_FOUND), cx);
    }

    /// The replace field and its Replace and Replace all buttons, in edit mode only.
    pub(in super::super) fn replace_row(&self, t: &Theme, cx: &mut Context<Self>) -> Option<Div> {
        let bar = self.find.bar.as_ref().filter(|_| self.editing)?;
        let label = tr("editor-replace-label");
        let one = Button::new("replace-one", tr("editor-replace-one"))
            .tooltip("Enter")
            .on_click(cx.listener(|view, _, window, cx| view.replace_one(window, cx)));
        let all = Button::new("replace-all", tr("editor-replace-all"))
            .tooltip("Ctrl+Alt+Enter")
            .on_click(cx.listener(|view, _, window, cx| view.replace_all_matches(window, cx)));
        let field = TextField::new("editor-replace-input", &bar.replace.input, label.clone());
        Some(
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(t.spacing.sm)
                .child(
                    div()
                        .flex_none()
                        .text_size(t.typography.small)
                        .text_color(t.colors.muted)
                        .child(label),
                )
                .child(
                    div()
                        .w(t.spacing.rail_width)
                        .min_w(px(MIN_INPUT))
                        .child(field),
                )
                .child(one)
                .child(all),
        )
    }
}
