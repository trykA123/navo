//! Composes the copied result into the writer a journal fill gives: the stage port of the copy.
//! It applies one hunk, picked lines or many hunks over the target and writes no file itself.

use std::collections::BTreeSet;
use std::io::Write;
use std::ops::Range;
use std::path::Path;

use navo_compare::text::side::LineEnding;
use navo_compare::text::{
    Alignment, ApplyError, Direction, Picked, PickedHunk, Row, TargetFacts, Whole, apply_picked,
};
use navo_fs::PagedFile;

use super::request::{CopyRefusal, CopyRequest, Shown, target_whole};
use crate::compare::diff::convert::hunk_rows;
use crate::compare::diff::load::Loaded;

#[cfg(test)]
mod tests;

/// The target and the source opened as paged files, before the pair is diffed again.
pub struct Paged {
    target: PagedFile,
    source: PagedFile,
}

/// One composed copy: the reloaded pair, its hunk indexes, picked lines and direction.
pub struct CopyInput<'a> {
    pub loaded: &'a Loaded,
    pub indices: &'a [usize],
    pub lines: &'a BTreeSet<usize>,
    pub direction: Direction,
}

/// Open the target, then the source, so a later change to either is seen by the apply.
/// Fails with `Failed` and the reason when a file cannot be opened.
pub fn open_both(target: &Path, source: &Path) -> Result<Paged, CopyRefusal> {
    let open = |path: &Path| PagedFile::open(path).map_err(CopyRefusal::Failed);
    Ok(Paged {
        target: open(target)?,
        source: open(source)?,
    })
}

/// Apply the whole change, or only the picked lines, into `out`.
/// An empty pick is `NotCopyable`; a file that moved under the read is `Changed`.
pub fn apply(
    request: &CopyRequest,
    shown: &Shown<'_>,
    files: &Paged,
    out: &mut dyn Write,
) -> Result<(), CopyRefusal> {
    let whole = BTreeSet::new();
    let lines = match &request.lines {
        None => &whole,
        Some(lines) if lines.is_empty() => return Err(CopyRefusal::NotCopyable),
        Some(lines) => lines,
    };
    apply_many(
        &CopyInput {
            loaded: shown.loaded,
            indices: &[shown.index],
            lines,
            direction: request.direction,
        },
        files,
        out,
    )
}

/// Apply several whole hunks, or their picked lines, in one pass over the target into `out`.
/// On an error `out` may hold a partial result that the caller discards.
pub fn apply_many(
    input: &CopyInput<'_>,
    files: &Paged,
    out: &mut dyn Write,
) -> Result<(), CopyRefusal> {
    let alignment = input
        .loaded
        .alignment
        .as_ref()
        .ok_or(CopyRefusal::NotCopyable)?;
    let facts = facts_of(target_whole(input.direction, input.loaded)?);
    let ranges = hunk_rows(alignment);
    let mut picked_hunks = Vec::with_capacity(input.indices.len());
    for index in input.indices {
        let hunk = alignment
            .hunks
            .get(*index)
            .ok_or(CopyRefusal::NotCopyable)?;
        let rows = rows_of(alignment, &ranges, *index, input.lines)?;
        picked_hunks.push(PickedHunk { hunk, rows });
    }
    let (left, right) = starts_of(input.loaded)?;
    let picked = Picked {
        facts,
        hunks: &picked_hunks,
        digests: &alignment.digests,
        left_starts: left,
        right_starts: right,
        lines: input.lines,
        direction: input.direction,
    };
    apply_picked(&files.target, &files.source, picked, out).map_err(refusal_of)
}

/// The alignment rows of one hunk, needed only when lines are picked rather than whole.
fn rows_of<'a>(
    alignment: &'a Alignment,
    ranges: &[Range<usize>],
    index: usize,
    lines: &BTreeSet<usize>,
) -> Result<&'a [Row], CopyRefusal> {
    if lines.is_empty() {
        return Ok(&[]);
    }
    let range = ranges.get(index).ok_or(CopyRefusal::NotCopyable)?;
    alignment
        .rows
        .get(range.clone())
        .ok_or(CopyRefusal::NotCopyable)
}

/// The line start offsets of the left and right sides, as the picked-lines apply needs.
fn starts_of(loaded: &Loaded) -> Result<(&[u64], &[u64]), CopyRefusal> {
    let left = loaded.left.as_ref().ok_or(CopyRefusal::NotCopyable)?;
    let right = loaded.right.as_ref().ok_or(CopyRefusal::NotCopyable)?;
    Ok((&left.starts, &right.starts))
}

/// The target's dominant line ending and whether it ends with a newline.
fn facts_of(target: &Whole) -> TargetFacts {
    let breaks = target
        .lines
        .iter()
        .filter(|line| line.ending != LineEnding::None)
        .count();
    let crlf = target
        .lines
        .iter()
        .filter(|line| line.ending == LineEnding::CrLf)
        .count();
    TargetFacts {
        dominant: if crlf > breaks - crlf {
            LineEnding::CrLf
        } else {
            LineEnding::Lf
        },
        final_newline: target
            .lines
            .last()
            .is_some_and(|line| line.ending != LineEnding::None),
    }
}

/// Map an apply error to a refusal: a changed file is "changed since shown".
fn refusal_of(error: ApplyError) -> CopyRefusal {
    match error {
        ApplyError::Changed => CopyRefusal::Changed,
        ApplyError::NotCopyable => CopyRefusal::NotCopyable,
        other => CopyRefusal::Failed(other.to_string()),
    }
}
