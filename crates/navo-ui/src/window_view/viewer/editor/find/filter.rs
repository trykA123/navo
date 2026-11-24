//! Filter Lines in the editor's find bar: keep, remove, copy or bookmark the lines that match
//! the find pattern (plain or regex, with Match case). Keep and Remove are one undo step each;
//! a text over 1 MiB is filtered on the background executor. The menu lives in `filter_menu.rs`.

use gpui_kit::ClipboardItem;
use navo_l10n::{t, tr};
use regex::Regex;

use super::TYPE_FIRST;
use crate::prelude::*;
use crate::window_view::viewer::editor::EditorView;
use crate::window_view::viewer::search::bookmark_all;

/// Texts longer than this, in bytes, are filtered off the UI thread.
const BACKGROUND_FROM: usize = 1 << 20;

/// What Filter Lines does with the matching lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in super::super) enum FilterAction {
    Keep,
    Remove,
    Copy,
    Bookmark,
}

impl FilterAction {
    pub(super) const ALL: [Self; 4] = [Self::Keep, Self::Remove, Self::Copy, Self::Bookmark];

    /// True for the actions that change the text, which need edit mode.
    pub(super) fn edits(self) -> bool {
        matches!(self, Self::Keep | Self::Remove)
    }

    /// The menu label, localised.
    pub(super) fn label(self) -> String {
        tr(match self {
            Self::Keep => "editor-filter-keep",
            Self::Remove => "editor-filter-remove",
            Self::Copy => "editor-filter-copy",
            Self::Bookmark => "editor-filter-bookmark",
        })
    }
}

/// The result of a filter run, ready to apply on the UI thread.
enum Outcome {
    /// The whole new text and how many lines matched.
    Text { next: String, matched: usize },
    /// The matching lines joined for the clipboard and their count.
    Copied { text: String, lines: usize },
    /// The byte offsets where the matching lines start.
    Starts(Vec<u64>),
}

/// A line without its `\n` or `\r\n` ending.
fn body(line: &str) -> &str {
    let line = line.strip_suffix('\n').unwrap_or(line);
    line.strip_suffix('\r').unwrap_or(line)
}

/// The lines of `text` with their endings, each with its start offset in bytes.
fn lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.split_inclusive('\n').scan(0, |at, line| {
        let start = *at;
        *at += line.len();
        Some((start, line))
    })
}

/// `text` with only the lines that match (`keep`) or only those that do not, each line keeping
/// its own ending; a text that ended without a line break still does. Also the match count.
fn filtered(text: &str, regex: &Regex, keep: bool) -> (String, usize) {
    let mut next = String::with_capacity(text.len());
    let mut matched = 0;
    for (_, line) in lines(text) {
        let is_match = regex.is_match(body(line));
        matched += usize::from(is_match);
        if is_match == keep {
            next.push_str(line);
        }
    }
    if !text.ends_with('\n') && next.ends_with('\n') {
        next.truncate(body(&next).len());
    }
    (next, matched)
}

/// Runs `action` over `text`; `ending` joins copied lines.
fn outcome(action: FilterAction, regex: &Regex, text: &str, ending: &str) -> Outcome {
    let matching = || lines(text).filter(|(_, line)| regex.is_match(body(line)));
    match action {
        FilterAction::Keep | FilterAction::Remove => {
            let (next, matched) = filtered(text, regex, action == FilterAction::Keep);
            Outcome::Text { next, matched }
        }
        FilterAction::Copy => {
            let bodies: Vec<&str> = matching().map(|(_, line)| body(line)).collect();
            Outcome::Copied {
                lines: bodies.len(),
                text: bodies.join(ending),
            }
        }
        FilterAction::Bookmark => Outcome::Starts(matching().map(|(at, _)| at as u64).collect()),
    }
}

impl EditorView {
    /// Runs `action` with the find bar's pattern (or the last one) over the live text; shows
    /// why nothing happened when no pattern is typed, it does not compile or the view reads only.
    pub(in super::super) fn filter_lines(
        &mut self,
        action: FilterAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if action.edits() && !self.editing {
            return self.say_filter(tr("editor-filter-edit-first"), cx);
        }
        let Some(regex) = self.filter_regex(cx) else {
            return;
        };
        let text = self.editor.read(cx).value();
        let ending = if self.crlf { "\r\n" } else { "\n" };
        if text.len() <= BACKGROUND_FROM {
            let done = outcome(action, &regex, &text, ending);
            return self.apply_filter(done, window, cx);
        }
        let revision = self.revision;
        self.find.filtering = Some(cx.spawn_in(window, async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { outcome(action, &regex, &text, ending) })
                .await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.finish_filter((revision, done), window, cx);
            });
        }));
    }

    /// Applies a background run unless the text changed since it started (`revision`).
    fn finish_filter(
        &mut self,
        (revision, done): (u64, Outcome),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.find.filtering = None;
        if self.revision == revision {
            self.apply_filter(done, window, cx);
        }
    }

    /// The typed (or last) pattern compiled as regex, or escaped in plain mode; `None` after
    /// showing why.
    fn filter_regex(&mut self, cx: &mut Context<Self>) -> Option<Regex> {
        let typed = match &self.find.bar {
            Some(_) => self.typed(cx),
            None => self.find.last.clone(),
        };
        let Some(text) = typed else {
            self.say_filter(TYPE_FIRST.into(), cx);
            return None;
        };
        if self.find.regex {
            return self.live_regex(&text, cx);
        }
        self.live_regex(&regex::escape(&text), cx)
    }

    /// Shows `note` in the find bar when it is open, else in the header.
    fn say_filter(&mut self, note: String, cx: &mut Context<Self>) {
        if self.find.bar.is_some() {
            return self.set_find_error(Some(note.as_str()), cx);
        }
        self.note = Some(note.into());
        cx.notify();
    }

    /// Applies a finished run: one whole-text replace (one undo step), the clipboard or the
    /// bookmarks; "Not found." when no line matched.
    fn apply_filter(&mut self, done: Outcome, window: &mut Window, cx: &mut Context<Self>) {
        if is_empty(&done) {
            return self.say_filter(super::NOT_FOUND.into(), cx);
        }
        match done {
            Outcome::Text { next, .. } => {
                if *self.editor.read(cx).value() != *next {
                    self.editor
                        .update(cx, |state, cx| state.replace_all(next, window, cx));
                }
                self.sync_live_source(cx);
                self.set_find_error(None, cx);
            }
            Outcome::Copied { text, lines } => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                self.note_filter(t!("editor-filter-copied", count = lines), cx);
            }
            Outcome::Starts(starts) => {
                let count = starts.len();
                bookmark_all(self, &starts, cx);
                self.note_filter(t!("editor-filter-bookmarked", count = count), cx);
            }
        }
    }

    /// Clears the bar's error and shows `note` in the header.
    fn note_filter(&mut self, note: String, cx: &mut Context<Self>) {
        self.set_find_error(None, cx);
        self.note = Some(note.into());
        cx.notify();
    }
}

/// True when the outcome found no matching line.
fn is_empty(done: &Outcome) -> bool {
    match done {
        Outcome::Text { matched, .. } => *matched == 0,
        Outcome::Copied { lines, .. } => *lines == 0,
        Outcome::Starts(starts) => starts.is_empty(),
    }
}
