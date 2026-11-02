use std::collections::BTreeSet;
use std::ops::Range;

use navo_core::encoding::{DetectedBy, Encoding, TextEncoding};
use navo_core::paging::{ByteSource, Laid, TextLayout};

use super::super::diff::{Alignment, Hunk, Row};
use super::super::side::{Line, LineEnding, LineSet, detect};
use super::{ApplyError, Direction, Picked, PickedHunk, TargetFacts, apply_hunk, apply_picked};

fn encoding(label: &[u8]) -> &'static Encoding {
    Encoding::for_label(label).unwrap()
}

fn laid(bytes: &[u8]) -> Laid<Bytes<'_>> {
    Laid::new(Bytes(bytes), detect(bytes, encoding(b"utf-8")))
}

struct Bytes<'a>(&'a [u8]);

impl ByteSource for Bytes<'_> {
    fn len(&self) -> u64 {
        u64::try_from(self.0.len()).unwrap()
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        <[u8] as ByteSource>::read_at(self.0, offset, buf)
    }
}

fn view(bytes: &[u8]) -> Laid<Bytes<'_>> {
    Laid::new(Bytes(bytes), TextLayout::default())
}

#[derive(Debug)]
struct Side {
    bytes: Vec<u8>,
    starts: Vec<u64>,
    lines: Vec<Line>,
    bom: bool,
}

impl Side {
    fn new(lines: Vec<(String, LineEnding)>) -> Self {
        let mut bytes = Vec::new();
        let mut starts = vec![0u64];
        let mut stored = Vec::new();
        for (text, ending) in lines {
            bytes.extend_from_slice(text.as_bytes());
            bytes.extend_from_slice(ending.as_str().as_bytes());
            starts.push(u64::try_from(bytes.len()).unwrap());
            stored.push(Line {
                text,
                ending,
                bom: false,
            });
        }
        Self {
            bytes,
            starts,
            lines: stored,
            bom: false,
        }
    }

    fn with_bom(lines: Vec<(String, LineEnding)>) -> Self {
        let mut side = Self::new(lines);
        side.bytes.splice(0..0, [0xEF, 0xBB, 0xBF]);
        for start in &mut side.starts {
            *start += 3;
        }
        if let Some(line) = side.lines.first_mut() {
            line.bom = true;
        }
        side.bom = true;
        side
    }

    fn set(&self) -> LineSet<'_> {
        LineSet {
            lines: &self.lines,
            starts: &self.starts,
            bom: self.bom,
        }
    }
}

fn lines(spec: &[(&str, LineEnding)]) -> Side {
    Side::new(
        spec.iter()
            .map(|(text, ending)| ((*text).to_owned(), *ending))
            .collect(),
    )
}

fn bom_lines(spec: &[(&str, LineEnding)]) -> Side {
    Side::with_bom(
        spec.iter()
            .map(|(text, ending)| ((*text).to_owned(), *ending))
            .collect(),
    )
}

fn utf16le(spec: &[(&str, LineEnding)]) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for (text, ending) in spec {
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        for unit in ending.as_str().encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
    }
    bytes
}

fn wide_side(spec: &[(&str, LineEnding)]) -> Side {
    let bytes = utf16le(spec);
    let mut starts = vec![2u64];
    let mut stored = Vec::new();
    for (index, (text, ending)) in spec.iter().enumerate() {
        let text_len = 2 * u64::try_from(text.encode_utf16().count()).unwrap();
        let ending_len = 2 * u64::try_from(ending.as_str().encode_utf16().count()).unwrap();
        starts.push(starts.last().unwrap() + text_len + ending_len);
        stored.push(Line {
            text: (*text).to_owned(),
            ending: *ending,
            bom: index == 0,
        });
    }
    Side {
        bytes,
        starts,
        lines: stored,
        bom: true,
    }
}

fn utf16le_view(bytes: &[u8]) -> Laid<Bytes<'_>> {
    let layout = TextLayout::of(TextEncoding {
        encoding: encoding(b"utf-16le"),
        bom: false,
        by: DetectedBy::Chosen,
    });
    Laid::new(Bytes(bytes), layout)
}

fn windows_view(bytes: &[u8]) -> Laid<Bytes<'_>> {
    let layout = TextLayout::of(TextEncoding {
        encoding: encoding(b"windows-1252"),
        bom: false,
        by: DetectedBy::Chosen,
    });
    Laid::new(Bytes(bytes), layout)
}

fn facts_of(lines: &[Line]) -> TargetFacts {
    let breaks = lines
        .iter()
        .filter(|line| line.ending != LineEnding::None)
        .count();
    let crlf = lines
        .iter()
        .filter(|line| line.ending == LineEnding::CrLf)
        .count();
    let dominant = if crlf > breaks - crlf {
        LineEnding::CrLf
    } else {
        LineEnding::Lf
    };
    TargetFacts {
        dominant,
        final_newline: lines
            .last()
            .is_some_and(|line| line.ending != LineEnding::None),
    }
}

fn line_range(starts: &[u64], lines: Range<usize>) -> Range<u64> {
    starts[lines.start]..starts[lines.end]
}

fn whole_hunk(
    left_lines: Range<usize>,
    left_starts: &[u64],
    right_lines: Range<usize>,
    right_starts: &[u64],
) -> Hunk {
    Hunk {
        left_bytes: line_range(left_starts, left_lines.clone()),
        right_bytes: line_range(right_starts, right_lines.clone()),
        left_lines,
        right_lines,
    }
}

fn apply_whole(left: &Side, right: &Side, hunk: &Hunk, direction: Direction) -> Vec<u8> {
    let (target, source) = if direction == Direction::LeftToRight {
        (right, left)
    } else {
        (left, right)
    };
    let mut out = Vec::new();
    apply_hunk(
        &view(&target.bytes),
        &view(&source.bytes),
        hunk,
        direction,
        facts_of(&target.lines),
        &mut out,
    )
    .unwrap();
    out
}

fn in_hunk(row: &Row, hunk: &Hunk) -> bool {
    match *row {
        Row::Changed { left, right } | Row::Equal { left, right } => {
            hunk.left_lines.contains(&left) && hunk.right_lines.contains(&right)
        }
        Row::Deleted { left } => hunk.left_lines.contains(&left),
        Row::Inserted { right } => hunk.right_lines.contains(&right),
    }
}

fn row_sets(alignment: &Alignment) -> Vec<Vec<Row>> {
    alignment
        .hunks
        .iter()
        .map(|hunk| {
            alignment
                .rows
                .iter()
                .filter(|row| in_hunk(row, hunk))
                .copied()
                .collect()
        })
        .collect()
}

struct Case<'a> {
    left: &'a Side,
    right: &'a Side,
    alignment: &'a Alignment,
    rows: &'a [Vec<Row>],
    picks: &'a BTreeSet<usize>,
    direction: Direction,
}

fn apply_to(case: &Case<'_>, out: &mut Vec<u8>) -> Result<(), ApplyError> {
    let (target, source) = if case.direction == Direction::LeftToRight {
        (case.right, case.left)
    } else {
        (case.left, case.right)
    };
    let entries: Vec<PickedHunk<'_>> = case
        .alignment
        .hunks
        .iter()
        .zip(case.rows)
        .map(|(hunk, rows)| PickedHunk {
            hunk,
            rows: rows.as_slice(),
        })
        .collect();
    apply_picked(
        &view(&target.bytes),
        &view(&source.bytes),
        Picked {
            facts: facts_of(&target.lines),
            hunks: &entries,
            left_starts: &case.left.starts,
            right_starts: &case.right.starts,
            lines: case.picks,
            direction: case.direction,
        },
        out,
    )
}

fn apply_rows(case: &Case<'_>) -> Result<Vec<u8>, ApplyError> {
    let mut out = Vec::new();
    apply_to(case, &mut out)?;
    Ok(out)
}

fn picked(case: &Case<'_>) -> Vec<u8> {
    apply_rows(case).unwrap()
}

fn picks(lines: impl IntoIterator<Item = usize>) -> BTreeSet<usize> {
    lines.into_iter().collect()
}

mod changed;
mod limits;
mod oracle;
mod rows;
mod whole;
