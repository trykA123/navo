use std::ops::Range;
use std::sync::Arc;

use navo_core::paging::{LineIndex, TextLayout};

use crate::anchor::SectionPair;
use crate::model::Reason;
use crate::text::normalise::{Options, normalise};
use crate::text::side::{Line, LineSet};

use super::Source;
use super::budget::{
    self, Charge, Detail, Hunks, Limits, Out, StepReason, bytes_of, diff_slices, lines_bytes,
    work_estimate,
};
use super::read::{Lines, Until, collect, end_of, start_of};
use super::schedule::Beat;

mod bounds;

pub(super) type Stop<'a> = &'a (dyn Fn() -> bool + Sync);

pub(super) struct SideData {
    pub(super) source: Source,
    pub(super) layout: TextLayout,
    pub(super) len: u64,
    pub(super) lines: u64,
    pub(super) index: LineIndex,
}

/// Both sides of one attempt; the charge covers both line indexes.
pub(super) struct Context {
    pub(super) left: SideData,
    pub(super) right: SideData,
    pub(super) options: Options,
    pub(super) _charge: Charge,
}

/// One copy of a batch's section pairs, counted while any job still holds it.
pub(super) struct Sections {
    pub(super) list: Vec<SectionPair>,
    pub(super) charge: Charge,
}

#[derive(Clone)]
pub(super) struct Batch {
    pub(super) sections: Arc<Sections>,
    pub(super) start_anchor: bool,
    pub(super) end_anchor: bool,
    pub(super) terminal: bool,
}

impl Batch {
    pub(super) fn left(&self) -> Range<u64> {
        span(&self.sections.list, |section| &section.left)
    }

    pub(super) fn right(&self) -> Range<u64> {
        span(&self.sections.list, |section| &section.right)
    }

    pub(super) fn lines(&self) -> u64 {
        let (left, right) = (self.left(), self.right());
        (left.end - left.start).saturating_add(right.end - right.start)
    }

    fn pair(&self) -> SectionPair {
        SectionPair {
            left: self.left(),
            right: self.right(),
        }
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
    Covered { out: Out, counts: Counts },
    Collision { start: bool, end: bool },
    Retry,
}

/// What one worker needs for a job: the deadline beat and the pairs the watchdog gave up.
pub(super) struct Env<'a> {
    pub(super) context: &'a Context,
    pub(super) limits: &'a Limits,
    pub(super) stop: Stop<'a>,
    pub(super) beat: &'a Beat,
    pub(super) forced: &'a [SectionPair],
}

impl Env<'_> {
    /// Starts the deadline of one pair; true when the watchdog already gave it up.
    pub(super) fn begin(&self, pair: &SectionPair) -> bool {
        self.beat.start_section(pair);
        self.forced.iter().any(|given| covers(given, pair))
    }
}

pub(super) fn coarse_fallback(
    env: &Env<'_>,
    batch: &Batch,
    reason: StepReason,
) -> Result<Outcome, Reason> {
    let Some(mut out) = Out::with_blocks(env.limits, 1) else {
        return Ok(Outcome::Retry);
    };
    let pair = batch.pair();
    env.beat.start_section(&pair);
    let detail = budget::detail(env.context, &pair, env.stop);
    env.beat.end_section();
    out.coarse(&detail?, reason)?;
    Ok(Outcome::Covered {
        out,
        counts: Counts {
            sections: batch.sections.list.len(),
            dropped: 0,
        },
    })
}

fn covers(outer: &SectionPair, inner: &SectionPair) -> bool {
    outer.left.start <= inner.left.start
        && outer.left.end >= inner.left.end
        && outer.right.start <= inner.right.start
        && outer.right.end >= inner.right.end
}

pub(super) fn run(env: &Env<'_>, batch: &Batch) -> Result<Outcome, Reason> {
    if (env.stop)() {
        return Err(Reason::Stopped);
    }
    match reserve_window(env, batch)? {
        Some(charge) => run_window(env, batch, charge),
        None => bounds::run_sections(env, batch),
    }
}

fn reserve_window(env: &Env<'_>, batch: &Batch) -> Result<Option<Charge>, Reason> {
    let lead = u64::from(batch.start_anchor);
    let mut pair = batch.pair();
    pair.left.start = pair.left.start.saturating_sub(lead);
    pair.right.start = pair.right.start.saturating_sub(lead);
    env.beat.start_section(&batch.pair());
    let whole = budget::detail(env.context, &pair, env.stop);
    env.beat.end_section();
    let whole = whole?;
    if budget::over_cap(&whole).is_some() {
        return Ok(None);
    }
    Ok(env.limits.ledger.reserve(window_estimate(&whole)))
}

fn window_estimate(whole: &Detail) -> u64 {
    let lines =
        whole.pair.left.end - whole.pair.left.start + whole.pair.right.end - whole.pair.right.start;
    let bytes = whole.left_bytes.end - whole.left_bytes.start + whole.right_bytes.end
        - whole.right_bytes.start;
    work_estimate(lines, bytes)
}

struct Window {
    left: Lines,
    right: Lines,
    left_base: u64,
    right_base: u64,
}

impl Window {
    fn left_at(&self, line: u64) -> usize {
        usize::try_from(line - self.left_base).unwrap_or(usize::MAX)
    }

    fn right_at(&self, line: u64) -> usize {
        usize::try_from(line - self.right_base).unwrap_or(usize::MAX)
    }

    fn same(&self, left: u64, right: u64, options: &Options) -> bool {
        same_key(
            &self.left.lines[self.left_at(left)],
            &self.right.lines[self.right_at(right)],
            options,
        )
    }

    fn detail(&self, pair: &SectionPair) -> Detail {
        let (left, right) = (self.left_at(pair.left.start), self.left_at(pair.left.end));
        let (top, bottom) = (
            self.right_at(pair.right.start),
            self.right_at(pair.right.end),
        );
        Detail {
            pair: pair.clone(),
            left_bytes: self.left.starts[left]..self.left.starts[right],
            right_bytes: self.right.starts[top]..self.right.starts[bottom],
        }
    }
}

fn run_window(env: &Env<'_>, batch: &Batch, mut charge: Charge) -> Result<Outcome, Reason> {
    env.beat.start_section(&batch.pair());
    let window = read_window(env, batch, &mut charge);
    env.beat.end_section();
    let window = window?;
    let options = &env.context.options;
    let (left, right) = (batch.left(), batch.right());
    let start = batch.start_anchor && !window.same(left.start - 1, right.start - 1, options);
    let end = batch.end_anchor && !window.same(left.end - 1, right.end - 1, options);
    if start || end {
        return Ok(Outcome::Collision { start, end });
    }
    let terminal = batch.terminal && !window.same(left.end - 1, right.end - 1, options);
    let room = batch.sections.list.len();
    let merged = charge
        .try_add(bytes_of::<SectionPair>(room))
        .then(|| merge_window(batch, &window, options));
    let (sections, dropped) = match &merged {
        Some((list, dropped)) => (list.as_slice(), *dropped),
        None => (batch.sections.list.as_slice(), 0),
    };
    if let Some((list, _)) = &merged {
        charge.settle(
            charge
                .bytes()
                .saturating_add(bytes_of::<SectionPair>(list.capacity())),
        );
    }
    let Some(mut out) = Out::with_blocks(env.limits, sections.len()) else {
        return coarse_fallback(env, batch, StepReason::Budget);
    };
    for pair in sections {
        if (env.stop)() {
            return Err(Reason::Stopped);
        }
        window_section(env, &window, pair, &mut out)?;
        env.beat.end_section();
    }
    let counts = Counts {
        sections: sections.len(),
        dropped: dropped + usize::from(terminal),
    };
    Ok(Outcome::Covered { out, counts })
}

fn read_window(env: &Env<'_>, batch: &Batch, charge: &mut Charge) -> Result<Window, Reason> {
    let lead = u64::from(batch.start_anchor);
    let context = env.context;
    let (left_lines, right_lines) = (batch.left(), batch.right());
    let left = read_side(&context.left, left_lines.clone(), lead, env.stop)?;
    let right = read_side(&context.right, right_lines.clone(), lead, env.stop)?;
    let lines = u64::try_from(left.lines.len() + right.lines.len()).unwrap_or(u64::MAX);
    let bytes = text_span(&left).saturating_add(text_span(&right));
    let measured = lines_bytes(&left).saturating_add(lines_bytes(&right));
    charge.cover_lines(lines, bytes, measured);
    Ok(Window {
        left,
        right,
        left_base: left_lines.start - lead,
        right_base: right_lines.start - lead,
    })
}

fn text_span(lines: &Lines) -> u64 {
    let first = lines.starts.first().copied().unwrap_or(0);
    lines.starts.last().copied().unwrap_or(first) - first
}

fn window_section(
    env: &Env<'_>,
    window: &Window,
    pair: &SectionPair,
    out: &mut Out,
) -> Result<(), Reason> {
    let forced = env.begin(pair);
    let detail = window.detail(pair);
    if forced {
        out.coarse(&detail, StepReason::Watchdog)?;
        return Ok(());
    }
    if let Some(stall) = &env.limits.stall {
        stall(pair.left.start);
    }
    let context = env.context;
    let left = slice(
        &window.left,
        window.left_at(pair.left.start)..window.left_at(pair.left.end),
        context.left.layout.text.bom && pair.left.start == 0,
    );
    let right = slice(
        &window.right,
        window.right_at(pair.right.start)..window.right_at(pair.right.end),
        context.right.layout.text.bom && pair.right.start == 0,
    );
    match diff_slices(context, pair, left, right)? {
        Some(hunks) => {
            if let Some(before_hash) = &env.limits.before_hash {
                before_hash();
            }
            let Some((digests, charge)) =
                budget::hash_hunks(&window.left, &window.right, &hunks, &mut out.charge)?
            else {
                out.coarse(&detail, StepReason::Budget)?;
                return Ok(());
            };
            out.section(
                env.limits,
                &detail,
                Hunks {
                    hunks,
                    digests,
                    charge,
                },
            )?;
        }
        None => out.coarse(&detail, StepReason::TokenCap)?,
    }
    Ok(())
}

fn merge_window(batch: &Batch, window: &Window, options: &Options) -> (Vec<SectionPair>, usize) {
    let list = &batch.sections.list;
    let mut kept = Vec::with_capacity(list.len());
    let mut dropped = 0;
    let mut sections = list.iter();
    let Some(mut current) = sections.next().cloned() else {
        return (kept, dropped);
    };
    for next in sections {
        if window.same(current.left.end - 1, current.right.end - 1, options) {
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

fn slice(lines: &Lines, range: Range<usize>, bom: bool) -> LineSet<'_> {
    LineSet {
        lines: &lines.lines[range.clone()],
        starts: &lines.starts[range.start..=range.end],
        bom,
        bytes: None,
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
        end: end_of(side, lines.end, stop)?,
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
