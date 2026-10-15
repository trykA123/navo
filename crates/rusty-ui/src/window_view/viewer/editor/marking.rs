use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::Focusable;
use gpui_kit::base::input::{Rope, RopeExt};
use gpui_kit::component::input::{RangeDecoration, RangeDecorationStyle};
use rusty_core::paging::ByteSource;

use super::EditorView;
use crate::prelude::*;
use crate::tokens::Theme;
use crate::window_view::viewer::body::BodyAct;
use crate::window_view::viewer::files::Reopen;
use crate::window_view::viewer::search::find_all::{FoundRow, PanelRows, preview};
use crate::window_view::viewer::search::marks::{clear_marks, toggle_mark};
use crate::window_view::viewer::search::overview::TickScale;
use crate::window_view::viewer::search::{Marked, Marking, source_changed, toggle_bookmark};

const SEARCH_FIRST: &str = "Search first; then Ctrl+1…8 marks it.";
const NO_BOOKMARKS: &str = "No bookmarks yet. Ctrl+F2 sets one.";
const READ_ONLY: &str = "Line endings show while reading.";
const NO_FOLLOW: &str = "Follow works in the paged view of large text files.";
const RECOUNT_AFTER: Duration = Duration::from_millis(500);

struct TextBytes(Arc<str>);

impl ByteSource for TextBytes {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.0.as_bytes().read_at(offset, buf)
    }
}

pub(super) fn text_source(text: Arc<str>) -> Reopen {
    Arc::new(move || Ok(Box::new(TextBytes(text.clone())) as Box<dyn ByteSource + Send>))
}

impl Marked for EditorView {
    fn marking(&mut self) -> &mut Marking {
        &mut self.marking
    }

    fn jump(&mut self, hit: Range<u64>, _: &mut Window, cx: &mut Context<Self>) {
        let range = usize::try_from(hit.start).unwrap_or(0)..usize::try_from(hit.end).unwrap_or(0);
        self.editor
            .update(cx, |state, cx| state.set_selected_range(range, cx));
        self.find.hit = Some(hit.start);
    }

    fn refocus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focus = match &self.find.bar {
            Some(bar) => bar.input.read(cx).focus_handle(cx),
            None => self.editor.read(cx).focus_handle(cx),
        };
        window.focus(&focus, cx);
    }
}

fn line_start_at(rope: &Rope, offset: usize) -> u64 {
    rope.line_start_offset(rope.offset_to_point(offset).row) as u64
}

impl EditorView {
    pub(super) fn act(&mut self, act: BodyAct, cx: &mut Context<Self>) {
        match act {
            BodyAct::Mark(colour) => self.mark(colour, cx),
            BodyAct::ClearMarks => clear_marks(self, cx),
            BodyAct::ToggleBookmark => {
                let at = self.cursor_line(cx);
                toggle_bookmark(self, at, cx);
            }
            BodyAct::StepBookmark { backward } => self.step_bookmark(backward, cx),
            BodyAct::LineEnds => self.toggle_line_ends(cx),
            BodyAct::Follow => self.say(NO_FOLLOW, cx),
        }
    }

    fn say(&mut self, note: &'static str, cx: &mut Context<Self>) {
        self.note = Some(note.into());
        cx.notify();
    }

    fn cursor_line(&self, cx: &App) -> u64 {
        let state = self.editor.read(cx);
        line_start_at(state.text(), state.cursor())
    }

    fn mark(&mut self, colour: u8, cx: &mut Context<Self>) {
        let Some(text) = self.typed(cx).or_else(|| self.find.last.clone()) else {
            return self.say(SEARCH_FIRST, cx);
        };
        let needle = self.needle(&text);
        if let Err(error) = toggle_mark(self, needle, colour, cx) {
            self.note = Some(error.into());
            cx.notify();
        }
    }

    fn step_bookmark(&mut self, backward: bool, cx: &mut Context<Self>) {
        let line = self.cursor_line(cx);
        let bookmarks = &self.marking.bookmarks;
        let target = if backward {
            bookmarks.previous_before(line)
        } else {
            bookmarks.next_after(line)
        };
        let Some(at) = target.and_then(|at| usize::try_from(at).ok()) else {
            return self.say(NO_BOOKMARKS, cx);
        };
        self.editor
            .update(cx, |state, cx| state.set_selected_range(at..at, cx));
        cx.notify();
    }

    fn toggle_line_ends(&mut self, cx: &mut Context<Self>) {
        if self.editing {
            return self.say(READ_ONLY, cx);
        }
        self.marking.line_ends = !self.marking.line_ends;
        cx.notify();
    }

    pub(super) fn text_edited(&mut self, cx: &mut Context<Self>) {
        self.recount = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(RECOUNT_AFTER).await;
            let _ = this.update(cx, |view, cx| {
                let text: Arc<str> = Arc::from(view.editor.read(cx).text().to_string());
                view.marking.source = text_source(text);
                source_changed(view, cx);
            });
        }));
    }

    pub(super) fn sync_decorations(&mut self, t: &Theme, cx: &mut Context<Self>) {
        let key = (self.marking.marks.rev, t.mode);
        if self.decorated == Some(key) {
            return;
        }
        self.decorated = Some(key);
        let decorations = self
            .marking
            .marks
            .ranges_in(0..u64::MAX)
            .into_iter()
            .map(|(range, colour)| {
                let fill = t.marks.fills[usize::from(colour) % t.marks.fills.len()];
                let start = usize::try_from(range.start).unwrap_or(usize::MAX);
                let end = usize::try_from(range.end).unwrap_or(usize::MAX);
                RangeDecoration::new(start..end)
                    .with_style(RangeDecorationStyle::Fill)
                    .with_color(fill)
            })
            .collect();
        self.decorations.set(decorations, cx);
    }

    pub(super) fn update_overview(&mut self, t: &Theme, cx: &App) {
        let rope = self.editor.read(cx).text().clone();
        let place = |offset: u64| {
            rope.offset_to_point(usize::try_from(offset).unwrap_or(0))
                .row as u64
        };
        let scale = TickScale {
            len: rope.lines_len() as u64,
            rows: self.overview.rows(t),
            place: &place,
        };
        self.marking.update_ticks(&scale);
    }

    pub(super) fn overview_click(&mut self, y: Pixels, t: &Theme, cx: &mut Context<Self>) {
        let rows = self.overview.rows(t);
        if rows == 0 {
            return;
        }
        let row = self.overview.row_at(t, y);
        let rope = self.editor.read(cx).text().clone();
        let at = self.marking.tick_target(row).map_or_else(
            || {
                let line = rope.lines_len() * row as usize / rows as usize;
                rope.line_start_offset(line)
            },
            |offset| usize::try_from(offset).unwrap_or(0),
        );
        self.editor
            .update(cx, |state, cx| state.set_selected_range(at..at, cx));
    }

    pub(super) fn found_rows(&self, cx: &App) -> PanelRows {
        let rope = self.editor.read(cx).text().clone();
        let width = self.marking.find_all_width().unwrap_or(0);
        PanelRows {
            source: Rc::new(move |start, _| Some(found_in(&rope, start, width))),
            on_missing: None,
        }
    }
}

fn found_in(rope: &Rope, start: u64, width: u64) -> FoundRow {
    let offset = usize::try_from(start).unwrap_or(0).min(rope.len());
    let row = rope.offset_to_point(offset).row;
    let hit = offset - rope.line_start_offset(row);
    let width = usize::try_from(width).unwrap_or(0);
    let line = rope.slice_line(row).to_string();
    preview((row + 1).to_string(), &line, hit..hit + width)
}
