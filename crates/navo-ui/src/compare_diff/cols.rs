//! Display columns of the compare text (D39.1): widths, the widest line and the column cut.
//! Pure functions; the file diff bodies use them to shape only the visible slice of a line.

use std::ops::Range;

use unicode_width::UnicodeWidthChar;

use super::hooks::PairText;
use super::long::is_long;

/// Columns between tab stops.
pub const TAB_COLS: u32 = 4;

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

/// The column count of every line of a pair, and the widest. A line over `LONG_LINE` counts 0,
/// so it never sets the widest and wraps to one visual line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineCols {
    left: Vec<u32>,
    right: Vec<u32>,
    widest: u32,
}

/// The columns of each line of `lines`, with 0 for a line over `LONG_LINE`.
fn measure_side(lines: &[String]) -> Vec<u32> {
    let one = |line: &String| if is_long(line) { 0 } else { line_cols(line) };
    lines.iter().map(one).collect()
}

impl LineCols {
    /// Measures every line of `text`. It walks all bytes once, so call it off the UI thread.
    #[must_use]
    pub fn measure(text: &PairText) -> Self {
        let (left, right) = (measure_side(&text.left), measure_side(&text.right));
        let widest = left.iter().chain(&right).copied().max().unwrap_or(0);
        Self {
            left,
            right,
            widest,
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
    let mut walk = Walk {
        out: Shown {
            text: String::new(),
            runs: Vec::new(),
        },
        open: None,
        next_run: 0,
    };
    let (mut at, mut keep) = (0u32, cols.contains(&0));
    let mut buf = [0u8; 4];
    for (byte, ch) in line.char_indices() {
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
                " ".repeat(width as usize)
            } else {
                ch.encode_utf8(&mut buf).to_owned()
            };
            walk.push(runs, byte, &piece);
        }
        at = at.saturating_add(width);
    }
    walk.out
}

#[cfg(test)]
mod tests;
