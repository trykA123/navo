//! Stages the copied result in a file beside the target; the file is removed unless a job takes it.

use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use navo_compare::text::side::LineEnding;
use navo_compare::text::{
    ApplyError, Direction, Picked, PickedHunk, Row, TargetFacts, Whole, apply_picked,
};
use navo_core::{Location, ProviderPath};
use navo_fs::{PagedFile, os_path};

use super::super::load::Loaded;
use super::{CopyRefusal, CopyRequest, Shown, target_whole};
use crate::compare::diff::convert::hunk_rows;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// The target and the source opened as paged files, before the pair is diffed again.
pub(in crate::compare::diff) struct Paged {
    target: PagedFile,
    source: PagedFile,
}

/// One staged copy: the reloaded pair, its hunk indexes, picked lines and direction.
pub(in crate::compare::diff) struct CopyInput<'a> {
    pub loaded: &'a Loaded,
    pub indices: &'a [usize],
    pub lines: &'a BTreeSet<usize>,
    pub direction: Direction,
}

/// Open the target, then the source, so a later change to either is seen by the apply.
pub(in crate::compare::diff) fn open_both(
    target: &Path,
    source: &Path,
) -> Result<Paged, CopyRefusal> {
    let open = |path: &Path| PagedFile::open(path).map_err(CopyRefusal::Failed);
    Ok(Paged {
        target: open(target)?,
        source: open(source)?,
    })
}

/// A staged file. Dropping it removes the file, so every refusal leaves no temp file behind.
pub(in crate::compare::diff) struct Staged {
    path: PathBuf,
    location: Location,
    file: Option<File>,
}

impl Staged {
    /// Create an empty, uniquely named file in the target's folder.
    pub(in crate::compare::diff) fn create(target: &Location) -> Result<Self, CopyRefusal> {
        let (Some(folder), Some(name)) = target.parent_and_name() else {
            return Err(CopyRefusal::NotCopyable);
        };
        let unique = format!(
            ".{}.{}-{}.navo-part",
            name.to_text(),
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let location = folder
            .join(&ProviderPath::from_text(&unique))
            .map_err(|_| CopyRefusal::NotCopyable)?;
        let path = os_path(&location).map_err(|_| CopyRefusal::NotCopyable)?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| CopyRefusal::Failed(error.to_string()))?;
        Ok(Self {
            path,
            location,
            file: Some(file),
        })
    }

    /// Where the staged file lives.
    pub(in crate::compare::diff) fn location(&self) -> &Location {
        &self.location
    }

    /// Write the target with the change applied, flush it to disk and return its length in bytes.
    pub(super) fn fill(
        &mut self,
        request: &CopyRequest,
        shown: &Shown<'_>,
        files: &Paged,
    ) -> Result<u64, CopyRefusal> {
        let file = self.file.take().ok_or(CopyRefusal::NotCopyable)?;
        let mut out = BufWriter::new(file);
        apply(request, shown, files, &mut out)?;
        finish(out)
    }

    /// Write the target with every listed change applied, flush it and return its length in bytes.
    pub(in crate::compare::diff) fn fill_many(
        &mut self,
        input: &CopyInput<'_>,
        files: &Paged,
    ) -> Result<u64, CopyRefusal> {
        let file = self.file.take().ok_or(CopyRefusal::NotCopyable)?;
        let mut out = BufWriter::new(file);
        apply_many(input, files, &mut out)?;
        finish(out)
    }
}

/// Flush and sync a staged writer, then return its length in bytes.
fn finish(out: BufWriter<File>) -> Result<u64, CopyRefusal> {
    let file = out
        .into_inner()
        .map_err(|error| CopyRefusal::Failed(error.to_string()))?;
    file.sync_all()
        .map_err(|error| CopyRefusal::Failed(error.to_string()))?;
    file.metadata()
        .map(|metadata| metadata.len())
        .map_err(|error| CopyRefusal::Failed(error.to_string()))
}

impl Drop for Staged {
    /// Close the handle, then remove the file; a file a job already moved is not an error.
    fn drop(&mut self) {
        drop(self.file.take());
        if let Err(error) = fs::remove_file(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, "a staged compare copy was not removed");
        }
    }
}

/// Apply the whole change, or only the picked lines, with the apply engine.
fn apply(
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

/// Apply several whole hunks, or their picked lines, in one pass over the target.
fn apply_many(
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
    alignment: &'a navo_compare::text::Alignment,
    ranges: &[std::ops::Range<usize>],
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
