use std::ops::Range;

use navo_core::paging::{LineIndex, TextLayout};

use crate::anchor::SectionPair;
use crate::model::Reason;
use crate::text::diff::{Hunk, diff_lines};
use crate::text::normalise::{Options, normalise};
use crate::text::side::{Line, LineSet};

use super::read::{Lines, Until, collect, start_of};
use super::{Covered, Source};

pub(super) type Stop<'a> = &'a (dyn Fn() -> bool + Sync);

pub(super) struct SideData {
    pub(super) source: Source,
    pub(super) layout: TextLayout,
    pub(super) len: u64,
    pub(super) index: LineIndex,
}

pub(super) struct Context {
    pub(super) left: SideData,
    pub(super) right: SideData,
    pub(super) options: Options,
}

#[derive(Debug, Clone)]
pub(super) struct Batch {
    pub(super) sections: Vec<SectionPair>,
    pub(super) start_anchor: bool,
    pub(super) end_anchor: bool,
    pub(super) terminal: bool,
}

impl Batch {
    pub(super) fn left(&self) -> Range<u64> {
        span(&self.sections, |section| &section.left)
    }

    pub(super) fn right(&self) -> Range<u64> {
        span(&self.sections, |section| &section.right)
    }

    pub(super) fn lines(&self) -> u64 {
        let (left, right) = (self.left(), self.right());
        (left.end - left.start).saturating_add(right.end - right.start)
    }
}

fn span(sections: &[SectionPair], side: impl Fn(&SectionPair) -> &Range<u64>) -> Range<u64> {
    let start = sections.first().map_or(0, |section| side(section).start);
    let end = sections.last().map_or(start, |section| side(section).end);
    start..end
}

/// Sections and dropped anchors that one emitted batch contributes to the summary.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Counts {
    pub(super) sections: usize,
    pub(super) dropped: usize,
}

pub(super) enum Outcome {
    Covered { covered: Covered, counts: Counts },
    Collision { start: bool, end: bool },
}

pub(super) fn run(context: &Context, batch: &Batch, stop: Stop<'_>) -> Result<Outcome, Reason> {
    if stop() {
        return Err(Reason::Stopped);
    }
    let lead = u64::from(batch.start_anchor);
    let left = read_side(&context.left, batch.left(), lead, stop)?;
    let right = read_side(&context.right, batch.right(), lead, stop)?;
    let same = |left_at: usize, right_at: usize| {
        same_key(
            &left.lines[left_at],
            &right.lines[right_at],
            &context.options,
        )
    };
    let last = left.lines.len().saturating_sub(1)..right.lines.len().saturating_sub(1);
    let start = batch.start_anchor && !same(0, 0);
    let end = batch.end_anchor && !same(last.start, last.end);
    if start || end {
        return Ok(Outcome::Collision { start, end });
    }
    let terminal = batch.terminal && !same(last.start, last.end);
    let window = Window {
        left,
        right,
        base: Base {
            left: batch.left().start,
            right: batch.right().start,
            lead,
        },
    };
    let (sections, dropped) = drop_bad_anchors(batch, &window, &context.options);
    let counts = Counts {
        sections: sections.len(),
        dropped: dropped + usize::from(terminal),
    };
    let mut hunks = Vec::new();
    for section in &sections {
        if stop() {
            return Err(Reason::Stopped);
        }
        hunks.extend(diff_section(context, section, &window)?);
    }
    Ok(Outcome::Covered {
        covered: Covered {
            left: batch.left(),
            right: batch.right(),
            hunks,
        },
        counts,
    })
}

struct Window {
    left: Lines,
    right: Lines,
    base: Base,
}

struct Base {
    left: u64,
    right: u64,
    lead: u64,
}

impl Base {
    fn at(start: u64, line: u64, lead: u64) -> usize {
        usize::try_from(line - start + lead).unwrap_or(usize::MAX)
    }

    fn left_at(&self, line: u64) -> usize {
        Self::at(self.left, line, self.lead)
    }

    fn right_at(&self, line: u64) -> usize {
        Self::at(self.right, line, self.lead)
    }
}

fn drop_bad_anchors(
    batch: &Batch,
    window: &Window,
    options: &Options,
) -> (Vec<SectionPair>, usize) {
    let Window { left, right, base } = window;
    let mut kept = Vec::with_capacity(batch.sections.len());
    let mut dropped = 0;
    let mut sections = batch.sections.iter();
    let Some(mut current) = sections.next().cloned() else {
        return (kept, dropped);
    };
    for next in sections {
        let left_line = &left.lines[base.left_at(current.left.end - 1)];
        let right_line = &right.lines[base.right_at(current.right.end - 1)];
        if same_key(left_line, right_line, options) {
            kept.push(std::mem::replace(&mut current, next.clone()));
        } else {
            dropped += 1;
            current.left.end = next.left.end;
            current.right.end = next.right.end;
        }
    }
    kept.push(current);
    (kept, dropped)
}

fn diff_section(
    context: &Context,
    section: &SectionPair,
    window: &Window,
) -> Result<Vec<Hunk>, Reason> {
    let Window { left, right, base } = window;
    let left_range = base.left_at(section.left.start)..base.left_at(section.left.end);
    let right_range = base.right_at(section.right.start)..base.right_at(section.right.end);
    let left_set = slice(
        left,
        left_range,
        context.left.layout.text.bom && section.left.start == 0,
    );
    let right_set = slice(
        right,
        right_range,
        context.right.layout.text.bom && section.right.start == 0,
    );
    let left_shift = usize::try_from(section.left.start).map_err(|_| Reason::Unsupported)?;
    let right_shift = usize::try_from(section.right.start).map_err(|_| Reason::Unsupported)?;
    let mut hunks = diff_lines(left_set, right_set, &context.options).hunks;
    for hunk in &mut hunks {
        hunk.left_lines = hunk.left_lines.start + left_shift..hunk.left_lines.end + left_shift;
        hunk.right_lines = hunk.right_lines.start + right_shift..hunk.right_lines.end + right_shift;
    }
    Ok(hunks)
}

fn slice(lines: &Lines, range: Range<usize>, bom: bool) -> LineSet<'_> {
    LineSet {
        lines: &lines.lines[range.clone()],
        starts: &lines.starts[range.start..=range.end],
        bom,
    }
}

fn same_key(left: &Line, right: &Line, options: &Options) -> bool {
    normalise(&left.text, left.ending, left.bom, options)
        == normalise(&right.text, right.ending, right.bom, options)
}

fn read_side(
    side: &SideData,
    lines: Range<u64>,
    lead: u64,
    stop: Stop<'_>,
) -> Result<Lines, Reason> {
    unchanged(side)?;
    let first = lines.start - lead;
    let count = lines.end - first;
    let until = Until {
        offset: start_of(side, first, stop)?,
        lines: count,
        end: side.len,
        len: side.len,
    };
    let read = collect(side.source.as_ref(), side.layout, until, stop)?;
    if u64::try_from(read.lines.len()).ok() != Some(count) {
        return Err(Reason::ChangedOnDisk);
    }
    unchanged(side)?;
    Ok(read)
}

pub(super) fn unchanged(side: &SideData) -> Result<(), Reason> {
    let changed = side.source.take_changed();
    let rewritten = side.source.take_rewritten();
    if changed || rewritten || side.source.len() != side.len {
        return Err(Reason::ChangedOnDisk);
    }
    Ok(())
}
