use std::collections::BTreeSet;
use std::fmt;
use std::io::Write;
use std::ops::Range;

use navo_core::encoding::EncodeError;
use navo_core::paging::ByteSource;

use super::diff::{Hunk, Row};
use super::side::LineEnding;

mod rows;
mod source;
mod stable;
mod target;

use stable::Stable;
use target::{BUFFER, Target, Writer, copy_raw, copy_target};

/// The direction a copy runs: the source lines move onto the target side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    LeftToRight,
    RightToLeft,
}

impl Direction {
    pub(super) const fn source_is_left(self) -> bool {
        matches!(self, Self::LeftToRight)
    }
}

/// The target line facts from the same snapshot whose stamp the write checks (D9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetFacts {
    pub dominant: LineEnding,
    pub final_newline: bool,
}

/// Why a hunk could not be applied.
#[derive(Debug)]
pub enum ApplyError {
    Range,
    Read(String),
    Write(std::io::Error),
    Encode(EncodeError),
    Malformed,
    Changed,
}

impl fmt::Display for ApplyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Range => formatter.write_str("the hunk does not fit the file"),
            Self::Read(message) => write!(formatter, "reading the file failed: {message}"),
            Self::Write(error) => write!(formatter, "writing the result failed: {error}"),
            Self::Encode(error) => write!(formatter, "{error}"),
            Self::Malformed => formatter.write_str("the source text is not valid in its encoding"),
            Self::Changed => formatter.write_str("a file changed since the comparison"),
        }
    }
}

impl std::error::Error for ApplyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Write(error) => Some(error),
            Self::Encode(error) => Some(error),
            Self::Range | Self::Read(_) | Self::Malformed | Self::Changed => None,
        }
    }
}

impl From<EncodeError> for ApplyError {
    fn from(error: EncodeError) -> Self {
        Self::Encode(error)
    }
}

/// One picked hunk: its byte ranges and the alignment rows that cover it.
#[derive(Debug, Clone, Copy)]
pub struct PickedHunk<'a> {
    pub hunk: &'a Hunk,
    pub rows: &'a [Row],
}

/// The hunks, rows and source-side line numbers a copy writes.
#[derive(Debug, Clone, Copy)]
pub struct Picked<'a> {
    pub facts: TargetFacts,
    pub hunks: &'a [PickedHunk<'a>],
    pub left_starts: &'a [u64],
    pub right_starts: &'a [u64],
    /// Absolute source-side line numbers to copy.
    pub lines: &'a BTreeSet<usize>,
    pub direction: Direction,
}

/// Writes `target` with one whole hunk replaced by the matching source lines.
/// `out` is staging: on `Err` it may hold a partial result the caller discards.
#[expect(
    clippy::too_many_arguments,
    reason = "the J6 contract fixes this signature"
)]
pub fn apply_hunk(
    target: &dyn ByteSource,
    source: &dyn ByteSource,
    hunk: &Hunk,
    direction: Direction,
    facts: TargetFacts,
    out: &mut dyn Write,
) -> Result<(), ApplyError> {
    let lines = BTreeSet::new();
    let hunks = [PickedHunk { hunk, rows: &[] }];
    apply_picked(
        target,
        source,
        Picked {
            facts,
            hunks: &hunks,
            left_starts: &[],
            right_starts: &[],
            lines: &lines,
            direction,
        },
        out,
    )
}

/// Writes `target` with only the picked source lines copied; an empty set copies whole hunks.
/// `out` is staging: on `Err` it may hold a partial result the caller discards.
pub fn apply_picked(
    target: &dyn ByteSource,
    source: &dyn ByteSource,
    picked: Picked<'_>,
    out: &mut dyn Write,
) -> Result<(), ApplyError> {
    let stable = Stable::start(target, source)?;
    let result = write_picked(target, source, picked, out);
    stable.finish(result)
}

fn write_picked(
    target: &dyn ByteSource,
    source: &dyn ByteSource,
    picked: Picked<'_>,
    out: &mut dyn Write,
) -> Result<(), ApplyError> {
    let target = Target::new(target, picked.facts)?;
    if picked.lines.is_empty() {
        let whole = Whole {
            target: &target,
            source,
            direction: picked.direction,
        };
        whole.validate(picked.hunks)?;
        let mut buffer = vec![0u8; BUFFER];
        copy_raw(target.source, 0..target.bom_len, out, &mut buffer)?;
        let mut writer = Writer::new(out, target.layout, &target.dominant);
        whole.write(picked.hunks, &mut writer, &mut buffer)
    } else {
        let rows = rows::Rows {
            target: &target,
            source,
        };
        rows.validate(picked)?;
        let mut buffer = vec![0u8; BUFFER];
        copy_raw(target.source, 0..target.bom_len, out, &mut buffer)?;
        let mut writer = Writer::new(out, target.layout, &target.dominant);
        rows.write(picked, &mut writer, &mut buffer)
    }
}

pub(super) fn ranges(hunk: &Hunk, direction: Direction) -> (Range<u64>, Range<u64>) {
    if direction.source_is_left() {
        (hunk.right_bytes.clone(), hunk.left_bytes.clone())
    } else {
        (hunk.left_bytes.clone(), hunk.right_bytes.clone())
    }
}

struct Whole<'a, 't> {
    target: &'a Target<'t>,
    source: &'a dyn ByteSource,
    direction: Direction,
}

impl<'a, 't> Whole<'a, 't> {
    fn plan(&self, hunk: &Hunk, cursor: u64) -> Result<(Range<u64>, Range<u64>), ApplyError> {
        let (target_range, source_range) = ranges(hunk, self.direction);
        let start = target_range.start.max(self.target.bom_len);
        let end = target_range.end.max(start);
        if start < cursor
            || end > self.target.len
            || source_range.end < source_range.start
            || source_range.end > self.source.len()
        {
            return Err(ApplyError::Range);
        }
        Ok((start..end, source_range))
    }

    fn validate(&self, hunks: &[PickedHunk]) -> Result<(), ApplyError> {
        let mut cursor = self.target.bom_len;
        for entry in hunks {
            let (target_range, _) = self.plan(entry.hunk, cursor)?;
            cursor = target_range.end;
        }
        Ok(())
    }

    fn write(
        &self,
        hunks: &[PickedHunk],
        writer: &mut Writer<'t>,
        buffer: &mut [u8],
    ) -> Result<(), ApplyError> {
        let mut cursor = self.target.bom_len;
        for entry in hunks {
            let (target_range, source_range) = self.plan(entry.hunk, cursor)?;
            copy_target(self.target, cursor..target_range.start, writer, buffer)?;
            let eof = target_range.end == self.target.len && !self.target.final_newline;
            let copier = source::Copier::new(self.target, self.source, eof);
            copier.range(source_range, writer, buffer)?;
            cursor = target_range.end;
        }
        copy_target(self.target, cursor..self.target.len, writer, buffer)
    }
}

#[cfg(test)]
mod tests;
