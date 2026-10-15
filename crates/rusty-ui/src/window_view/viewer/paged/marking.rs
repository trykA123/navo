use std::ops::Range;

use gpui_kit::Focusable;
use rusty_core::paging::Needle;

use super::PagedView;
use super::find::BarKind;
use super::lines::PagedKind;
use super::moves::Move;
use crate::prelude::*;
use crate::window_view::viewer::body::BodyAct;
use crate::window_view::viewer::search::history::NeedleKind;
use crate::window_view::viewer::search::marks::{clear_marks, toggle_mark};
use crate::window_view::viewer::search::{Marked, Marking, toggle_bookmark};

const SEARCH_FIRST: &str = "Search first; then Ctrl+1…8 marks it.";
const NO_BOOKMARKS: &str = "No bookmarks yet. Ctrl+F2 sets one.";
const TEXT_ONLY: &str = "Line endings show in the text view.";

impl Marked for PagedView {
    fn marking(&mut self) -> &mut Marking {
        &mut self.marking
    }

    fn jump(&mut self, hit: Range<u64>, _: &mut Window, cx: &mut Context<Self>) {
        self.find.anchor = Some(hit.start);
        self.queue_move(Move::Offset(hit.start), cx);
        self.find.hit = Some(hit);
    }

    fn refocus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focus = match &self.find.bar {
            Some(bar) => bar.input.read(cx).focus_handle(cx),
            None => self.focus.clone(),
        };
        window.focus(&focus, cx);
    }
}

impl PagedView {
    pub(super) fn needle_kind(&self) -> NeedleKind {
        match self.kind {
            PagedKind::Hex => NeedleKind::Hex,
            PagedKind::Text => NeedleKind::Text,
        }
    }

    fn typed_or_last(&self, cx: &App) -> Option<String> {
        self.find
            .bar
            .as_ref()
            .filter(|bar| bar.kind == BarKind::Find)
            .map(|bar| bar.input.read(cx).value().to_string())
            .filter(|text| !text.is_empty())
            .or_else(|| self.find.last.clone())
    }

    fn marked_needle(&self, cx: &App) -> Option<Needle> {
        self.typed_or_last(cx)
            .and_then(|text| self.needle(&text).ok())
    }

    pub(in crate::window_view::viewer) fn act(&mut self, act: BodyAct, cx: &mut Context<Self>) {
        match act {
            BodyAct::Mark(colour) => self.mark(colour, cx),
            BodyAct::ClearMarks => clear_marks(self, cx),
            BodyAct::ToggleBookmark => toggle_bookmark(self, self.top, cx),
            BodyAct::StepBookmark { backward } => self.step_bookmark(backward, cx),
            BodyAct::LineEnds => self.toggle_line_ends(cx),
            BodyAct::Follow => self.toggle_follow(cx),
        }
    }

    fn mark(&mut self, colour: u8, cx: &mut Context<Self>) {
        let Some(needle) = self.marked_needle(cx) else {
            return self.set_note(SEARCH_FIRST, cx);
        };
        if let Err(error) = toggle_mark(self, needle, colour, cx) {
            self.set_note(error, cx);
        }
    }

    fn step_bookmark(&mut self, backward: bool, cx: &mut Context<Self>) {
        let bookmarks = &self.marking.bookmarks;
        let target = if backward {
            bookmarks.previous_before(self.top)
        } else {
            bookmarks.next_after(self.top)
        };
        match target {
            Some(at) => self.user_move(Move::Offset(at), cx),
            None => self.set_note(NO_BOOKMARKS, cx),
        }
    }

    fn toggle_line_ends(&mut self, cx: &mut Context<Self>) {
        if self.kind != PagedKind::Text {
            return self.set_note(TEXT_ONLY, cx);
        }
        self.marking.line_ends = !self.marking.line_ends;
        cx.notify();
    }
}
