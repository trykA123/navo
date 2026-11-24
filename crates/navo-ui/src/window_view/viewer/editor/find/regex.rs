//! Regex mode in the editor's find bar: compiling the pattern with size limits, stepping
//! through matches in the live text, and counting them on the background executor.
//! Replace uses it from `replace.rs`.

use std::ops::Range;
use std::time::Duration;

use gpui_kit::Task;
use navo_core::paging::{Counted, ordinal};
use navo_l10n::{t, tr};
use regex::{Regex, RegexBuilder};

use crate::format;
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant};
use crate::window_view::viewer::editor::EditorView;

/// How long typing must pause before the pattern is compiled and counted.
const TYPING_PAUSE: Duration = Duration::from_millis(150);

/// The most memory, in bytes, a compiled pattern and its lazy DFA may use.
const SIZE_LIMIT: usize = 1 << 20;

/// The most match starts a count keeps; past it the label shows the total with a `+`.
const MAX_STARTS: usize = 100_000;

/// What a count was made for: the pattern, Match case and the text revision it read.
type CountKey = (String, bool, u64);

/// The regex-mode match count over the live text and the background job making it.
#[derive(Default)]
pub(in super::super) struct RegexCount {
    /// The finished count; `None` while it runs or when no valid pattern is typed.
    pub counted: Option<Counted>,
    /// True when the count stopped at `MAX_STARTS` with more matches left.
    pub capped: bool,
    key: Option<CountKey>,
    /// Dropping the task cancels it, so a newer count replaces an older one.
    _job: Option<Task<()>>,
}

/// The count label for a capped count: "place of total+" on a kept match, else "total+ matches".
pub(super) fn capped_label(counted: &Counted, hit: Option<u64>) -> String {
    let shown = format::bytes(counted.total);
    match hit.and_then(|at| ordinal(counted, at)) {
        Some(place) => t!(
            "editor-regex-capped-place",
            place = format::bytes(place),
            shown = shown
        ),
        None => t!("editor-regex-capped", shown = shown),
    }
}

/// Compiles `pattern` with `^`/`$` matching at every line; the error is the text to show.
/// Fails when the pattern is not valid regex syntax or needs more than `SIZE_LIMIT`.
pub(super) fn compile(pattern: &str, match_case: bool) -> Result<Regex, String> {
    RegexBuilder::new(pattern)
        .case_insensitive(!match_case)
        .multi_line(true)
        .size_limit(SIZE_LIMIT)
        .dfa_size_limit(SIZE_LIMIT)
        .build()
        .map_err(|error| match error {
            regex::Error::Syntax(text) => {
                format!("{} {}", tr("editor-regex-invalid"), short(&text))
            }
            _ => tr("editor-regex-too-complex"),
        })
}

/// The one-line cause from the regex crate's multi-line syntax report (its `error:` line).
fn short(report: &str) -> &str {
    report
        .lines()
        .find_map(|line| line.trim().strip_prefix("error: "))
        .unwrap_or_else(|| report.lines().last().unwrap_or_default().trim())
}

/// The next byte offset after `at` that starts a char, or `None` at the end of `text`.
fn next_char(text: &str, at: usize) -> Option<usize> {
    text.get(at..)?.chars().next().map(|c| at + c.len_utf8())
}

/// The match after the selection `from`, wrapping to the top; an empty match on the
/// caret itself is skipped, so each step moves at least one char.
fn next_match(regex: &Regex, text: &str, from: Range<usize>) -> Option<Range<usize>> {
    let found = regex.find_at(text, from.end).map(|found| found.range());
    let found = match found {
        Some(range) if range.is_empty() && range == from => {
            next_char(text, from.end).and_then(|at| regex.find_at(text, at).map(|m| m.range()))
        }
        other => other,
    };
    found.or_else(|| regex.find(text).map(|found| found.range()))
}

/// The last match that starts before the selection `from`, wrapping to the bottom. The
/// walk stops at the first match at or past `from.start`; only a wrap walks to the end.
fn previous_match(regex: &Regex, text: &str, from: Range<usize>) -> Option<Range<usize>> {
    let mut matches = regex.find_iter(text);
    let mut before = None;
    for found in matches.by_ref() {
        if found.start() >= from.start {
            if before.is_some() {
                return before;
            }
            return Some(matches.last().unwrap_or(found).range());
        }
        before = Some(found.range());
    }
    before
}

/// Up to `MAX_STARTS` match starts in `text` as a finished count; the flag is true when
/// more matches follow, so the walk stops at match `MAX_STARTS + 1`.
fn count(regex: &Regex, text: &str) -> (Counted, bool) {
    let mut starts: Vec<u64> = regex
        .find_iter(text)
        .take(MAX_STARTS + 1)
        .map(|found| found.start() as u64)
        .collect();
    let capped = starts.len() > MAX_STARTS;
    starts.truncate(MAX_STARTS);
    let counted = Counted {
        total: starts.len() as u64,
        starts,
        done: true,
    };
    (counted, capped)
}

/// Recounts the typed pattern once typing pauses for `TYPING_PAUSE`.
pub(super) fn recount_after_pause(cx: &mut Context<EditorView>) -> Task<()> {
    cx.spawn(async move |this, cx| {
        cx.background_executor().timer(TYPING_PAUSE).await;
        let _ = this.update(cx, |view, cx| {
            view.set_find_error(None, cx);
            view.recount_typed(cx);
        });
    })
}

impl EditorView {
    /// The Regex toggle next to Match case: filled while regex mode is on.
    pub(in super::super) fn regex_button(&self, cx: &mut Context<Self>) -> Button {
        let variant = if self.find.regex {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Ghost
        };
        Button::new("find-regex", tr("editor-regex-button"))
            .variant(variant)
            .tooltip("Alt+R")
            .on_click(cx.listener(|view, _, _, cx| view.toggle_regex(cx)))
    }

    /// Turns regex mode on or off and recounts the typed pattern.
    pub(in super::super) fn toggle_regex(&mut self, cx: &mut Context<Self>) {
        self.find.regex = !self.find.regex;
        self.set_find_error(None, cx);
        self.recount_typed(cx);
    }

    /// Recounts the typed text in the current mode: regex now, plain through the scanner.
    pub(in super::super) fn recount_typed(&mut self, cx: &mut Context<Self>) {
        self.find.regex_count = RegexCount::default();
        if self.find.regex {
            let marking = &mut self.marking;
            marking.count = None;
            marking.rev += 1;
            if let Some(text) = self.typed(cx)
                && let Some(regex) = self.live_regex(&text, cx)
            {
                self.count_in_background(&text, regex, cx);
            }
        } else if let Some(needle) = self.typed_needle(cx) {
            crate::window_view::viewer::search::count::count_search(self, needle, cx);
        }
        cx.notify();
    }

    /// Compiles `pattern`; on a bad pattern it shows the error, clears the count and
    /// returns `None`. It never counts.
    pub(super) fn live_regex(&mut self, pattern: &str, cx: &mut Context<Self>) -> Option<Regex> {
        let compiled = compile(pattern, self.find.match_case);
        if let Err(error) = &compiled {
            self.find.regex_count = RegexCount::default();
            self.set_find_error(Some(error), cx);
        }
        compiled.ok()
    }

    /// Counts `regex` over the live text on the background executor, unless a count for
    /// the same pattern, Match case and text revision is already shown or running.
    fn count_in_background(&mut self, pattern: &str, regex: Regex, cx: &mut Context<Self>) {
        let key = (pattern.to_owned(), self.find.match_case, self.revision);
        if self.find.regex_count.key.as_ref() == Some(&key) {
            return;
        }
        let text = self.editor.read(cx).value();
        let wanted = key.clone();
        let job = cx.spawn(async move |this, cx| {
            let counted = cx
                .background_executor()
                .spawn(async move { count(&regex, &text) })
                .await;
            let _ = this.update(cx, |view, cx| view.show_regex_count(&wanted, counted, cx));
        });
        self.find.regex_count = RegexCount {
            key: Some(key),
            _job: Some(job),
            ..RegexCount::default()
        };
    }

    /// Shows a finished count unless a newer count replaced it or the text changed since.
    fn show_regex_count(
        &mut self,
        key: &CountKey,
        (counted, capped): (Counted, bool),
        cx: &mut Context<Self>,
    ) {
        let count = &mut self.find.regex_count;
        if count.key.as_ref() != Some(key) || key.2 != self.revision {
            return;
        }
        count.counted = Some(counted);
        count.capped = capped;
        cx.notify();
    }

    /// Selects the next (or previous) regex match in the live text; shows why when none.
    pub(super) fn regex_search(&mut self, pattern: &str, backward: bool, cx: &mut Context<Self>) {
        let Some(regex) = self.live_regex(pattern, cx) else {
            self.find.hit = None;
            return;
        };
        self.count_in_background(pattern, regex.clone(), cx);
        let hit = self.editor.update(cx, |state, cx| {
            let text = state.value();
            let from = state.selected_range();
            let hit = if backward {
                previous_match(&regex, &text, from)
            } else {
                next_match(&regex, &text, from)
            };
            if let Some(range) = &hit {
                state.set_selected_range(range.clone(), cx);
            }
            hit
        });
        self.find.hit = hit.as_ref().map(|range| range.start as u64);
        self.set_find_error(hit.is_none().then_some(super::NOT_FOUND), cx);
    }

    /// Replaces the selection when it is a match of `pattern`, expanding `$1`-style groups
    /// from `with`, then selects the next match.
    pub(super) fn regex_replace_one(
        &mut self,
        (pattern, with): (&str, &str),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(regex) = self.live_regex(pattern, cx) else {
            return;
        };
        self.editor.update(cx, |state, cx| {
            let text = state.value();
            let selected = state.selected_range();
            let Some(caps) = regex.captures_at(&text, selected.start) else {
                return;
            };
            if caps.get(0).map(|whole| whole.range()) == Some(selected) {
                let mut expanded = String::new();
                caps.expand(with, &mut expanded);
                state.replace(expanded, window, cx);
            }
        });
        self.regex_search(pattern, false, cx);
    }

    /// Rewrites every match of `pattern` in the live text with `with` (groups expanded) as
    /// one whole-text replace, so one undo restores it; shows "Not found." when none.
    pub(super) fn regex_replace_all(
        &mut self,
        (pattern, with): (&str, &str),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(regex) = self.live_regex(pattern, cx) else {
            return;
        };
        let changed = self.editor.update(cx, |state, cx| {
            let text = state.value();
            let std::borrow::Cow::Owned(next) = regex.replace_all(&text, with) else {
                return false;
            };
            state.replace_all(next, window, cx);
            true
        });
        self.sync_live_source(cx);
        self.set_find_error((!changed).then_some(super::NOT_FOUND), cx);
    }
}
