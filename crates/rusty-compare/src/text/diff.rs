use std::ops::Range;

use blake3::Hasher;
use imara_diff::{Algorithm, Diff as ImaraDiff, InternedInput, TokenSource};

use crate::model::CaseRule;

use super::normalise::{Options, Whitespace};
use super::side::{DIFF_TOKEN_LIMIT, Line, LineSet};

const HASH_BYTES: usize = 16;

/// One aligned visual row. `E8` emits `Equal`, `Deleted` and `Inserted`;
/// `Changed` is left for E9's similarity pairing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Row {
    Equal {
        left: usize,
        right: usize,
    },
    /// A paired changed line. E9 fills these; E8 keeps deletes and inserts apart.
    Changed {
        left: usize,
        right: usize,
    },
    Deleted {
        left: usize,
    },
    Inserted {
        right: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fallback {
    TokenCap,
}

/// A changed run. Byte ranges are file-absolute source bytes: the first start
/// includes a BOM, and line endings are counted as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub left_lines: Range<usize>,
    pub right_lines: Range<usize>,
    pub left_bytes: Range<u64>,
    pub right_bytes: Range<u64>,
}

/// The aligned lines and hunks, with the raw and rule-normalised equality flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alignment {
    pub rows: Vec<Row>,
    pub hunks: Vec<Hunk>,
    pub raw_equal: bool,
    pub display_equal: bool,
    pub fallback: Option<Fallback>,
}

impl Alignment {
    fn equal(left: usize, right: usize, raw_equal: bool) -> Self {
        Self {
            rows: (0..left.min(right))
                .map(|index| Row::Equal {
                    left: index,
                    right: index,
                })
                .collect(),
            hunks: Vec::new(),
            raw_equal,
            display_equal: true,
            fallback: None,
        }
    }
}

/// Diff two decoded line slices with Histogram over hashes of their normalised keys.
///
/// The line and byte ranges are file-absolute when the caller passes the
/// `Whole` starts; a section can pass a sliced starts list. The BOM is included
/// in the first start, and line endings are counted as the side stores them.
#[must_use]
pub fn diff_lines(left: LineSet<'_>, right: LineSet<'_>, options: &Options) -> Alignment {
    diff_with_limit(left, right, options, DIFF_TOKEN_LIMIT)
}

fn diff_with_limit(
    left: LineSet<'_>,
    right: LineSet<'_>,
    options: &Options,
    limit: usize,
) -> Alignment {
    debug_assert_eq!(left.starts.len(), left.lines.len() + 1);
    debug_assert_eq!(right.starts.len(), right.lines.len() + 1);
    let raw_equal = left.lines == right.lines && left.bom == right.bom;
    if raw_equal {
        return Alignment::equal(left.lines.len(), right.lines.len(), true);
    }
    if left.lines.len().saturating_add(right.lines.len()) > limit {
        return fallback(left, right);
    }
    let left_hashes = hashes(left.lines, options);
    let right_hashes = hashes(right.lines, options);
    let display_equal =
        left_hashes == right_hashes && (options.ignore_bom || left.bom == right.bom);
    if display_equal {
        return Alignment::equal(left.lines.len(), right.lines.len(), false);
    }
    let input = InternedInput::new(
        KeySource { keys: &left_hashes },
        KeySource {
            keys: &right_hashes,
        },
    );
    let diff = ImaraDiff::compute(Algorithm::Histogram, &input);
    build_alignment(
        SideInfo {
            len: left.lines.len(),
            starts: left.starts,
        },
        SideInfo {
            len: right.lines.len(),
            starts: right.starts,
        },
        &diff,
    )
}

struct SideInfo<'a> {
    len: usize,
    starts: &'a [u64],
}

fn build_alignment(
    left_info: SideInfo<'_>,
    right_info: SideInfo<'_>,
    diff: &ImaraDiff,
) -> Alignment {
    let mut rows = Vec::new();
    let mut hunks = Vec::new();
    let (mut left, mut right) = (0, 0);
    for hunk in diff.hunks() {
        let left_start = usize::try_from(hunk.before.start).unwrap_or(usize::MAX);
        let left_end = usize::try_from(hunk.before.end).unwrap_or(usize::MAX);
        let right_start = usize::try_from(hunk.after.start).unwrap_or(usize::MAX);
        let right_end = usize::try_from(hunk.after.end).unwrap_or(usize::MAX);
        while left < left_start && right < right_start {
            rows.push(Row::Equal { left, right });
            left += 1;
            right += 1;
        }
        for index in left_start..left_end {
            rows.push(Row::Deleted { left: index });
        }
        for index in right_start..right_end {
            rows.push(Row::Inserted { right: index });
        }
        hunks.push(Hunk {
            left_lines: left_start..left_end,
            right_lines: right_start..right_end,
            left_bytes: byte_range(left_info.starts, left_start, left_end),
            right_bytes: byte_range(right_info.starts, right_start, right_end),
        });
        left = left_end;
        right = right_end;
    }
    while left < left_info.len && right < right_info.len {
        rows.push(Row::Equal { left, right });
        left += 1;
        right += 1;
    }
    Alignment {
        rows,
        hunks,
        raw_equal: false,
        display_equal: false,
        fallback: None,
    }
}

fn fallback(left: LineSet<'_>, right: LineSet<'_>) -> Alignment {
    let mut rows = Vec::with_capacity(left.lines.len() + right.lines.len());
    for index in 0..left.lines.len() {
        rows.push(Row::Deleted { left: index });
    }
    for index in 0..right.lines.len() {
        rows.push(Row::Inserted { right: index });
    }
    Alignment {
        rows,
        hunks: vec![Hunk {
            left_lines: 0..left.lines.len(),
            right_lines: 0..right.lines.len(),
            left_bytes: 0..total_bytes(left.starts),
            right_bytes: 0..total_bytes(right.starts),
        }],
        raw_equal: false,
        display_equal: false,
        fallback: Some(Fallback::TokenCap),
    }
}

fn hashes(lines: &[Line], options: &Options) -> Vec<[u8; HASH_BYTES]> {
    let mut hashes = Vec::with_capacity(lines.len());
    for line in lines {
        hashes.push(key_hash(line, options));
    }
    hashes
}

fn key_hash(line: &Line, options: &Options) -> [u8; HASH_BYTES] {
    let mut hasher = Hasher::new();
    if line.bom && !options.ignore_bom {
        hasher.update("\u{feff}".as_bytes());
    }
    let text = if options.whitespace == Whitespace::Trailing {
        line.text.trim_end()
    } else {
        line.text.as_str()
    };
    match options.case {
        CaseRule::Sensitive => feed_text(&mut hasher, text, options.whitespace),
        CaseRule::Insensitive => feed_folded(&mut hasher, text, options.whitespace),
    }
    if !options.ignore_line_endings {
        hasher.update(line.ending.as_str().as_bytes());
    }
    let hash = hasher.finalize();
    let mut key = [0u8; HASH_BYTES];
    key.copy_from_slice(&hash.as_bytes()[..HASH_BYTES]);
    key
}

fn feed_text(hasher: &mut Hasher, text: &str, whitespace: Whitespace) {
    match whitespace {
        Whitespace::Keep | Whitespace::Trailing => {
            hasher.update(text.as_bytes());
        }
        Whitespace::All => {
            for character in text.chars().filter(|character| !character.is_whitespace()) {
                update_char(hasher, character);
            }
        }
        Whitespace::Changes => {
            let mut pending = false;
            for character in text.chars() {
                feed_character(hasher, character, whitespace, &mut pending);
            }
        }
    }
}

fn feed_folded(hasher: &mut Hasher, text: &str, whitespace: Whitespace) {
    let mut pending = false;
    for character in text.chars() {
        for upper in character.to_uppercase() {
            for lower in upper.to_lowercase() {
                feed_character(hasher, lower, whitespace, &mut pending);
            }
        }
    }
}

fn feed_character(
    hasher: &mut Hasher,
    character: char,
    whitespace: Whitespace,
    pending: &mut bool,
) {
    if whitespace == Whitespace::Changes {
        if character.is_whitespace() {
            *pending = true;
            return;
        }
        if *pending {
            hasher.update(b" ");
            *pending = false;
        }
    } else if whitespace == Whitespace::All && character.is_whitespace() {
        return;
    }
    update_char(hasher, character);
}

fn update_char(hasher: &mut Hasher, character: char) {
    let mut buffer = [0u8; 4];
    hasher.update(character.encode_utf8(&mut buffer).as_bytes());
}

fn byte_range(starts: &[u64], start: usize, end: usize) -> Range<u64> {
    let start = starts.get(start).copied().unwrap_or(0);
    let end = starts.get(end).copied().unwrap_or(start);
    start..end
}

fn total_bytes(starts: &[u64]) -> u64 {
    starts.last().copied().unwrap_or(0)
}

struct KeySource<'a> {
    keys: &'a [[u8; HASH_BYTES]],
}

impl<'a> TokenSource for KeySource<'a> {
    type Token = [u8; HASH_BYTES];
    type Tokenizer = std::iter::Copied<std::slice::Iter<'a, [u8; HASH_BYTES]>>;

    fn tokenize(&self) -> Self::Tokenizer {
        self.keys.iter().copied()
    }

    fn estimate_tokens(&self) -> u32 {
        u32::try_from(self.keys.len()).unwrap_or(u32::MAX)
    }
}

#[cfg(test)]
mod tests;
