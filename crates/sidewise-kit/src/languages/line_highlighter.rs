//! Editor adapter core: colours a rope for the code editor from a `LineLexer`.
//! It keeps a line state every 64 lines so a `styles` call lexes only a bounded stretch (D11).

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::base::input::EditorState;
use gpui_kit::base::input::{
    FoldRange, HighlightStyleResolver, InputEdit, InputHighlighter, InputHighlighterFactory, Rope,
    RopeExt,
};
use gpui_kit::{Context, HighlightStyle, SharedString, Window};
use navo_core::lines::{LineLexer, LineState, MAX_LINE, Span};

/// Lines between two stored checkpoints.
const MARK_EVERY: usize = 64;
/// Most checkpoints kept (D11).
const MAX_MARKS: usize = 65_536;
/// Most runs one `spans` call returns (D6).
const MAX_RUNS: usize = 65_536;
/// Furthest distance (bytes) a call lexes from a checkpoint before it restarts nearby (D11).
const MAX_CATCH_UP: usize = 1024 * 1024;
/// How far before the range a restarted lex begins (bytes, D11).
const RESTART_BACK: usize = 64 * 1024;

/// Line-lexer colours over a rope with checkpoints every 64 lines.
pub struct LineHighlighter {
    lexer: Arc<dyn LineLexer>,
    text: Rope,
    marks: RefCell<Vec<LineState>>,
}

impl LineHighlighter {
    /// Builds an empty highlighter for `lexer`.
    pub fn new(lexer: Arc<dyn LineLexer>) -> Self {
        Self {
            lexer,
            text: Rope::new(),
            marks: RefCell::new(Vec::new()),
        }
    }

    /// Replaces the text; checkpoints after `from_row / 64` are dropped.
    pub fn set_text(&mut self, text: &Rope, from_row: usize) {
        self.text = text.clone();
        self.marks.get_mut().truncate(from_row / MARK_EVERY + 1);
    }

    /// Returns absolute, clipped, ordered spans inside `range` (at most 65 536).
    ///
    /// `range` must start and end on char boundaries of the text; GPUI always passes such ranges.
    /// A line-local lexer starts at the first requested row; any other lexer starts at a
    /// checkpoint or restarts near the range.
    pub fn spans(&self, range: &Range<usize>) -> Vec<Span> {
        let len = self.text.len();
        let range = range.start.min(len)..range.end.min(len);
        let mut out = Vec::new();
        if range.is_empty() {
            return out;
        }
        let first = self.text.offset_to_point(range.start).row;
        let last = self.text.offset_to_point(range.end).row;
        let (from, state, store) = if self.lexer.line_local() {
            (first, LineState::default(), false)
        } else {
            self.start_for(first, range.start)
        };
        let walk = Walk {
            from,
            state,
            store,
            last,
        };
        self.lex_rows(walk, &range, &mut out);
        out
    }

    /// Picks the row, state and whether to store checkpoints for a lex that must reach `first`.
    fn start_for(&self, first: usize, range_start: usize) -> (usize, LineState, bool) {
        let mut marks = self.marks.borrow_mut();
        if marks.is_empty() {
            marks.push(LineState::default());
        }
        let k = (first / MARK_EVERY).min(marks.len() - 1);
        let from = k * MARK_EVERY;
        let distance = self.text.line_start_offset(first) - self.text.line_start_offset(from);
        if distance <= MAX_CATCH_UP {
            return (from, marks[k], true);
        }
        let restart = self
            .text
            .offset_to_point(range_start.saturating_sub(RESTART_BACK))
            .row;
        (restart, LineState::default(), false)
    }

    /// Lexes the rows of `walk`, storing checkpoints when asked, and collects spans in `range`.
    fn lex_rows(&self, walk: Walk, range: &Range<usize>, out: &mut Vec<Span>) {
        let Walk {
            from,
            mut state,
            store,
            last,
        } = walk;
        let rows = self.text.lines_len();
        for row in from..=last.min(rows.saturating_sub(1)) {
            let start = self.text.line_start_offset(row);
            if start >= range.end && start > 0 {
                break;
            }
            if store {
                self.store_mark(row, state);
            }
            // Length comes from the rope offsets, before any slice is made (D6).
            if self.text.line_start_offset(row + 1) - start > MAX_LINE {
                continue;
            }
            let line = self.text.slice_line(row).to_string();
            // Every editor line is one chunk that ends its line.
            let (spans, end) = self.lexer.lex_last(&line, state);
            state = self.lexer.line_end(end);
            clip_into(&spans, start, range, out);
            if out.len() >= MAX_RUNS {
                out.truncate(MAX_RUNS);
                return;
            }
        }
    }

    /// Stores the state at the start of `row` when it is the next unstored checkpoint line.
    fn store_mark(&self, row: usize, state: LineState) {
        let mut marks = self.marks.borrow_mut();
        if row.is_multiple_of(MARK_EVERY)
            && row / MARK_EVERY == marks.len()
            && marks.len() < MAX_MARKS
        {
            marks.push(state);
        }
    }
}

impl LineHighlighter {
    /// Builds an editor highlighter factory that serves any language name with `lexer`.
    pub fn factory(lexer: Arc<dyn LineLexer>) -> InputHighlighterFactory {
        Rc::new(move |_: &str| {
            let highlighter: Box<dyn InputHighlighter> = Box::new(Self::new(lexer.clone()));
            Some(highlighter)
        })
    }
}

impl InputHighlighter for LineHighlighter {
    /// Returns the lexer's language name.
    fn language(&self) -> SharedString {
        SharedString::from(self.lexer.name().to_string())
    }

    /// Takes the new text; checkpoints after the edit row are dropped.
    fn update(
        &mut self,
        edit: Option<InputEdit>,
        text: &Rope,
        _folding: bool,
        _window: &mut Window,
        _cx: &mut Context<EditorState>,
    ) {
        self.set_text(text, edit.map_or(0, |e| e.start_position.row));
    }

    /// Returns ordered runs covering `range` exactly; gaps and unknown roles are default.
    /// At most `MAX_RUNS` runs: the last one is a default run for the rest of the range.
    fn styles(
        &self,
        range: &Range<usize>,
        resolver: &dyn HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let mut runs = Vec::new();
        let mut at = range.start;
        for (span, role) in self.spans(range) {
            // Count the runs this span needs (gap, span) and the tail that must still fit after it.
            let gap = usize::from(span.start > at);
            let tail = usize::from(span.end < range.end);
            if runs.len() + gap + 1 + tail > MAX_RUNS {
                break;
            }
            push_run(&mut runs, at..span.start, HighlightStyle::default());
            let style = resolver.style(role).unwrap_or_default();
            push_run(&mut runs, span.start..span.end, style);
            at = span.end;
        }
        push_run(&mut runs, at..range.end, HighlightStyle::default());
        runs
    }

    /// Returns no fold ranges: line lexers do not fold.
    fn fold_ranges(&self, _text: &Rope) -> Vec<FoldRange> {
        Vec::new()
    }
}

/// Appends a run, skipping empty ones and merging a default run into a default neighbour.
fn push_run(
    runs: &mut Vec<(Range<usize>, HighlightStyle)>,
    run: Range<usize>,
    style: HighlightStyle,
) {
    if run.is_empty() {
        return;
    }
    if style == HighlightStyle::default()
        && let Some((last, last_style)) = runs.last_mut()
        && *last_style == style
        && last.end == run.start
    {
        last.end = run.end;
        return;
    }
    runs.push((run, style));
}

/// Where a lex starts and ends, and whether it stores checkpoints.
struct Walk {
    from: usize,
    state: LineState,
    store: bool,
    last: usize,
}

/// Moves line-relative `spans` to absolute offsets, clipped to `range`, onto `out`.
fn clip_into(spans: &[Span], line_start: usize, range: &Range<usize>, out: &mut Vec<Span>) {
    for (r, role) in spans {
        let start = (line_start + r.start).max(range.start);
        let end = (line_start + r.end).min(range.end);
        if start < end {
            out.push((start..end, role));
        }
    }
}

#[cfg(test)]
mod tests;
