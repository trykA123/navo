use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::Focusable;
use gpui_kit::base::input::{Rope, RopeExt};
use gpui_kit::component::input::{RangeDecoration, RangeDecorationStyle};
use navo_core::paging::ByteSource;

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
const EDIT_SEARCH: &str =
    "Ctrl+1…8 cannot read the edit search. Mark it from the viewer search before editing.";
const NO_BOOKMARKS: &str = "No bookmarks yet. Ctrl+F2 sets one.";
const READ_ONLY: &str = "Line endings show while reading.";
const NO_FOLLOW: &str = "Follow works in the paged view of large text files.";
const RECOUNT_AFTER: Duration = Duration::from_millis(500);

struct TextBytes(Rope);

impl ByteSource for TextBytes {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        let total = self.0.len();
        let Ok(at) = usize::try_from(offset) else {
            return Ok(0);
        };
        if at >= total || buf.is_empty() {
            return Ok(0);
        }
        let (chunks, chunk_start) = self.0.chunks_at(at);
        let mut skip = at - chunk_start;
        let mut copied = 0;
        for chunk in chunks {
            let available = &chunk.as_bytes()[skip..];
            skip = 0;
            let take = available.len().min(buf.len() - copied);
            buf[copied..copied + take].copy_from_slice(&available[..take]);
            copied += take;
            if copied == buf.len() {
                break;
            }
        }
        Ok(copied)
    }
}

pub(super) fn text_source(text: Rope) -> Reopen {
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
            BodyAct::LineEnds => self.say(READ_ONLY, cx),
            BodyAct::Follow => self.say(NO_FOLLOW, cx),
        }
    }

    fn say(&mut self, note: &'static str, cx: &mut Context<Self>) {
        self.note = Some(note.into());
        cx.notify();
    }

    pub(in crate::window_view::viewer) fn cursor_row(&self, cx: &App) -> u64 {
        let state = self.editor.read(cx);
        state.text().offset_to_point(state.cursor()).row as u64
    }

    fn cursor_line(&self, cx: &App) -> u64 {
        let state = self.editor.read(cx);
        line_start_at(state.text(), state.cursor())
    }

    fn mark(&mut self, colour: u8, cx: &mut Context<Self>) {
        if self.editing {
            return self.say(EDIT_SEARCH, cx);
        }
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

    pub(super) fn text_edited(&mut self, cx: &mut Context<Self>) {
        self.recount = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(RECOUNT_AFTER).await;
            let _ = this.update(cx, |view, cx| {
                view.recount = None;
                view.refresh_source(cx);
            });
        }));
    }

    /// Rescans the live text now when an edit is still waiting for its recount, so the
    /// match count and Find all never read the text as it was before the edit.
    pub(super) fn sync_live_source(&mut self, cx: &mut Context<Self>) {
        if self.recount.take().is_some() {
            self.refresh_source(cx);
        }
    }

    /// Points the count, marks and Find all at the live text and recounts; in regex mode
    /// the regex count is redone too.
    fn refresh_source(&mut self, cx: &mut Context<Self>) {
        let text = self.editor.read(cx).text().clone();
        self.marking.source = text_source(text);
        source_changed(self, None, cx);
        if self.find.regex {
            self.recount_typed(cx);
        }
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

const BEFORE_HIT: usize = 256;
const AFTER_HIT: usize = 512;

fn char_floor(rope: &Rope, at: usize) -> usize {
    rope.char_index_to_offset(rope.offset_to_char_index(at))
}

fn char_ceil(rope: &Rope, at: usize) -> usize {
    let floor = char_floor(rope, at);
    if floor == at {
        return at;
    }
    rope.char_index_to_offset(rope.offset_to_char_index(at) + 1)
}

fn found_in(rope: &Rope, start: u64, width: u64) -> FoundRow {
    let offset = usize::try_from(start).unwrap_or(0).min(rope.len());
    let row = rope.offset_to_point(offset).row;
    let (line_start, line_end) = (rope.line_start_offset(row), rope.line_end_offset(row));
    let width = usize::try_from(width).unwrap_or(0);
    let from = char_floor(rope, line_start.max(offset.saturating_sub(BEFORE_HIT)));
    let to = char_ceil(rope, line_end.min(offset + width + AFTER_HIT)).max(from);
    let window = rope.slice(from..to).to_string();
    let hit = offset - from;
    preview((row + 1).to_string(), &window, hit..hit + width)
}

#[cfg(test)]
mod tests {
    use gpui_kit::base::input::Rope;
    use navo_core::paging::ByteSource;

    use super::{TextBytes, found_in};

    #[test]
    fn text_bytes_reads_across_rope_chunks_and_mid_utf8_offsets() {
        let content = "αβγ\n".repeat(4000);
        let source = TextBytes(Rope::from_str(&content));
        assert_eq!(source.len(), content.len() as u64);
        let mut buf = [0_u8; 777];
        let offset = 1025_u64;
        let read = source.read_at(offset, &mut buf).expect("read");
        assert_eq!(read, buf.len());
        assert_eq!(&buf[..], &content.as_bytes()[1025..1025 + 777]);
        assert_eq!(source.read_at(source.len(), &mut buf).expect("eof"), 0);
    }

    #[test]
    fn a_hit_deep_in_a_long_line_shows_only_a_window_around_it() {
        let mut line = "é".repeat(200_000);
        line.push_str("needle");
        line.push_str(&"x".repeat(100_000));
        let rope = Rope::from(line.as_str());
        let row = found_in(&rope, 400_000, 6);
        assert!(row.text.len() < 1024, "{} bytes", row.text.len());
        assert_eq!(&row.text[row.hit.clone()], "needle");
        assert_eq!(row.place.as_ref(), "1");
    }
}
