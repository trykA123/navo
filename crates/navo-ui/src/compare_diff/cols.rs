//! Display columns of the compare text (D39.1): widths, the widest line and the column cut.
//! Pure functions; the file diff bodies use them to shape only the visible slice of a line.

use std::ops::Range;

use unicode_width::UnicodeWidthChar;

use super::hooks::PairText;
use super::long::is_long;

mod wrap;
pub use wrap::WrapIndex;

/// Columns between tab stops.
pub const TAB_COLS: u32 = 4;

/// Columns between the checkpoints of a long line.
const CHECK_EVERY: u32 = 1024;

/// A line records checkpoints only once a character starts at or past this column.
const CHECK_FROM: u32 = 4096;

/// The spaces a tab or a control character draws: never more than `TAB_COLS` of them.
const SPACES: &str = "    ";

/// The columns character `ch` takes when it starts at column `at`: a tab runs to the next stop,
/// a control character takes 1 (drawn as a space), a wide one 2 and a combining mark 0.
#[must_use]
pub fn char_cols(ch: char, at: u32) -> u32 {
    if ch == '\t' {
        TAB_COLS - at % TAB_COLS
    } else if ch.is_control() {
        1
    } else {
        ch.width().map_or(0, |w| w as u32)
    }
}

/// Whether every byte of `line` is printable ASCII, so each takes one column.
fn is_plain(line: &str) -> bool {
    line.bytes().all(|b| (0x20..0x7f).contains(&b))
}

/// The columns of `line`, counted character by character.
fn slow_cols(line: &str) -> u32 {
    line.chars()
        .fold(0u32, |at, ch| at.saturating_add(char_cols(ch, at)))
}

/// The columns of `line`; saturates at `u32::MAX`. Printable ASCII skips the per-character walk.
#[must_use]
pub fn line_cols(line: &str) -> u32 {
    if is_plain(line) {
        u32::try_from(line.len()).unwrap_or(u32::MAX)
    } else {
        slow_cols(line)
    }
}

/// The visual lines a line of `cols` columns takes in a column `wrap` columns wide (at least 1).
#[must_use]
pub fn visual_lines(cols: u32, wrap: u32) -> u32 {
    cols.div_ceil(wrap.max(1)).max(1)
}

/// A point of a line the cut can resume from: the character at byte `byte` starts at column
/// `col`, so the tab stops and the kept columns read the same as they do from byte 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checkpoint {
    /// The display column of the character at `byte`.
    pub col: u32,
    /// The byte of that character in the line; always a character start.
    pub byte: usize,
}

/// The checkpoints of one line, in column order.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Marks {
    line: u32,
    points: Vec<Checkpoint>,
}

/// The column count of every line of a pair, and the widest. A line over `LONG_LINE` counts 0,
/// so it never sets the widest and wraps to one visual line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineCols {
    left: Vec<u32>,
    right: Vec<u32>,
    widest: u32,
    /// The checkpoints of the long lines of each side (left, then right), by line index.
    marks: [Vec<Marks>; 2],
}

/// The columns of each line of `lines` (0 over `LONG_LINE`) and the checkpoints of its long
/// lines, in one walk.
fn measure_side(lines: &[String]) -> (Vec<u32>, Vec<Marks>) {
    let mut cols = Vec::with_capacity(lines.len());
    let mut marks = Vec::new();
    for (at, line) in lines.iter().enumerate() {
        let (width, points) = measure_line(line);
        cols.push(width);
        if !points.is_empty() {
            let line = u32::try_from(at).unwrap_or(u32::MAX);
            marks.push(Marks { line, points });
        }
    }
    (cols, marks)
}

/// The columns of `line` and its checkpoints; a line over `LONG_LINE` counts 0 and records none.
fn measure_line(line: &str) -> (u32, Vec<Checkpoint>) {
    if is_long(line) {
        return (0, Vec::new());
    }
    if is_plain(line) {
        let cols = u32::try_from(line.len()).unwrap_or(u32::MAX);
        return (cols, plain_marks(cols));
    }
    slow_marks(line)
}

/// The checkpoints of a printable ASCII line of `cols` columns, where every byte is a column.
fn plain_marks(cols: u32) -> Vec<Checkpoint> {
    (CHECK_FROM..cols)
        .step_by(CHECK_EVERY as usize)
        .map(|col| Checkpoint {
            col,
            byte: col as usize,
        })
        .collect()
}

/// The columns of `line`, recording a checkpoint every `CHECK_EVERY` columns past `CHECK_FROM`.
/// Only a character that takes columns starts one, so a cut resuming there reads the same kept
/// state as a walk from byte 0 (a zero-width mark keeps the state of its base).
fn slow_marks(line: &str) -> (u32, Vec<Checkpoint>) {
    let mut points = Vec::new();
    let (mut at, mut next) = (0u32, CHECK_FROM);
    for (byte, ch) in line.char_indices() {
        let width = char_cols(ch, at);
        if width > 0 && at >= next {
            points.push(Checkpoint { col: at, byte });
            next = at.saturating_add(CHECK_EVERY);
        }
        at = at.saturating_add(width);
    }
    (at, points)
}

impl LineCols {
    /// Measures every line of `text`. It walks all bytes once, so call it off the UI thread.
    #[must_use]
    pub fn measure(text: &PairText) -> Self {
        let (left, left_marks) = measure_side(&text.left);
        let (right, right_marks) = measure_side(&text.right);
        let widest = left.iter().chain(&right).copied().max().unwrap_or(0);
        Self {
            left,
            right,
            widest,
            marks: [left_marks, right_marks],
        }
    }

    /// The columns of left line `line`, or 0 out of range.
    #[must_use]
    pub fn left(&self, line: u32) -> u32 {
        self.left.get(line as usize).copied().unwrap_or(0)
    }

    /// The columns of right line `line`, or 0 out of range.
    #[must_use]
    pub fn right(&self, line: u32) -> u32 {
        self.right.get(line as usize).copied().unwrap_or(0)
    }

    /// The widest line of either side, in columns.
    #[must_use]
    pub fn widest(&self) -> u32 {
        self.widest
    }

    /// The last checkpoint of left line `line` at or before column `col`; `None` walks from 0.
    #[must_use]
    pub fn left_start(&self, line: u32, col: u32) -> Option<Checkpoint> {
        start_at(&self.marks[0], line, col)
    }

    /// The last checkpoint of right line `line` at or before column `col`; `None` walks from 0.
    #[must_use]
    pub fn right_start(&self, line: u32, col: u32) -> Option<Checkpoint> {
        start_at(&self.marks[1], line, col)
    }
}

/// The checkpoint of `line` nearest at or below `col`, from marks ordered by line.
fn start_at(marks: &[Marks], line: u32, col: u32) -> Option<Checkpoint> {
    let at = marks.partition_point(|m| m.line < line);
    let points = &marks.get(at).filter(|m| m.line == line)?.points;
    let before = points.partition_point(|p| p.col <= col);
    before.checked_sub(1).map(|i| points[i])
}

/// A cut line: the text to draw and its runs, as byte ranges of that text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown<S> {
    pub text: String,
    pub runs: Vec<(Range<usize>, S)>,
}

/// Where the cut is while it walks a line.
struct Walk<S> {
    out: Shown<S>,
    /// The run index the last kept character belonged to, and where its output run sits.
    open: Option<(usize, usize)>,
    next_run: usize,
}

impl<S: Clone> Walk<S> {
    /// Adds `piece` (the drawn form of the character at input byte `at`) and extends its run.
    fn push(&mut self, runs: &[(Range<usize>, S)], at: usize, piece: &str) {
        let start = self.out.text.len();
        self.out.text.push_str(piece);
        while self.next_run < runs.len() && runs[self.next_run].0.end <= at {
            self.next_run += 1;
        }
        let Some((range, style)) = runs.get(self.next_run) else {
            return;
        };
        if !range.contains(&at) {
            return;
        }
        match self.open {
            Some((run, slot)) if run == self.next_run => {
                self.out.runs[slot].0.end = self.out.text.len();
            }
            _ => {
                self.open = Some((self.next_run, self.out.runs.len()));
                self.out
                    .runs
                    .push((start..self.out.text.len(), style.clone()));
            }
        }
    }
}

/// Cuts `line` to the display columns `cols` and returns the text to draw.
///
/// It keeps the characters whose start column is in `cols`, so a tab or wide character that
/// straddles `cols.start` is dropped and replaced by spaces in front: shown column 0 is always
/// `cols.start`. Tabs become spaces, controls one space, and a zero-width mark stays with its
/// base. `runs` are ordered, non-overlapping byte ranges of `line` on character boundaries; they
/// move with their characters and are clipped (padding spaces carry no run).
#[must_use]
pub fn cut<S: Clone>(line: &str, runs: &[(Range<usize>, S)], cols: Range<u32>) -> Shown<S> {
    cut_from(line, runs, cols, None)
}

/// Like [`cut`], resuming from `start` instead of byte 0. `start` must be a checkpoint of `line`
/// (from [`LineCols::left_start`] or [`LineCols::right_start`]) at or before `cols.start`; the
/// text and runs are the same as a walk from 0.
#[must_use]
pub fn cut_from<S: Clone>(
    line: &str,
    runs: &[(Range<usize>, S)],
    cols: Range<u32>,
    start: Option<Checkpoint>,
) -> Shown<S> {
    walk_line(line, runs, cols, start).0
}

/// Like [`cut_from`], also returning the characters examined, so a test outside this module can
/// bound the walk of a cut a body built.
#[cfg(test)]
pub(crate) fn cut_examined<S: Clone>(
    line: &str,
    runs: &[(Range<usize>, S)],
    cols: Range<u32>,
    start: Option<Checkpoint>,
) -> (Shown<S>, usize) {
    walk_line(line, runs, cols, start)
}

/// The cut and the number of characters it examined, so a test can bound the walk.
fn walk_line<S: Clone>(
    line: &str,
    runs: &[(Range<usize>, S)],
    cols: Range<u32>,
    start: Option<Checkpoint>,
) -> (Shown<S>, usize) {
    let start = start.unwrap_or(Checkpoint { col: 0, byte: 0 });
    let mut walk = Walk {
        out: Shown {
            text: String::new(),
            runs: Vec::new(),
        },
        open: None,
        next_run: 0,
    };
    let (mut at, mut keep) = (start.col, cols.contains(&start.col));
    let mut seen = 0usize;
    let rest = line.get(start.byte..).unwrap_or_default();
    for (byte, ch) in rest.char_indices() {
        seen += 1;
        let byte = byte + start.byte;
        let width = char_cols(ch, at);
        if width > 0 {
            if at >= cols.end {
                break;
            }
            keep = cols.contains(&at);
            if keep && walk.out.text.is_empty() {
                let pad = (at - cols.start) as usize;
                walk.out.text.extend(std::iter::repeat_n(' ', pad));
            }
        }
        if keep {
            let piece = if ch == '\t' || ch.is_control() {
                &SPACES[..width as usize]
            } else {
                &line[byte..byte + ch.len_utf8()]
            };
            walk.push(runs, byte, piece);
        }
        at = at.saturating_add(width);
    }
    (walk.out, seen)
}

#[cfg(test)]
mod tests;
