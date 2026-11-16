use std::ops::Range;
use std::sync::Arc;

use crate::anchor::SectionPair;
use crate::model::{CancelToken, Reason};
use crate::text::diff::{Hunk, diff_lines};
use crate::text::digest::HunkDigests;
use crate::text::side::{DIFF_TOKEN_LIMIT, LineSet};

use super::super::batch::{Batch, Context, Env, SideData, Stop, unchanged};
use super::super::plan::Hook;
use super::super::read::{Lines, Until, collect, end_of, start_of};
use super::hash::hash_hunks;
use super::sections::refine;
use super::{
    Charge, Ledger, Out, StepReason, bytes_of, lines_bytes, raw_bytes, span, work_estimate,
};

const SECTION_MAX_LINES: u64 = 1_000_000;
const SECTION_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// A final section pair with its exact absolute byte spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Detail {
    pub(crate) pair: SectionPair,
    pub(crate) left_bytes: Range<u64>,
    pub(crate) right_bytes: Range<u64>,
}

impl Detail {
    fn lines(&self) -> u64 {
        span(&self.pair.left).saturating_add(span(&self.pair.right))
    }

    fn bytes(&self) -> u64 {
        span(&self.left_bytes).saturating_add(span(&self.right_bytes))
    }
}

pub(crate) fn detail(
    context: &Context,
    pair: &SectionPair,
    stop: Stop<'_>,
) -> Result<Detail, Reason> {
    Ok(Detail {
        pair: pair.clone(),
        left_bytes: side_span(&context.left, &pair.left, stop)?,
        right_bytes: side_span(&context.right, &pair.right, stop)?,
    })
}

pub(crate) fn over_cap(detail: &Detail) -> Option<StepReason> {
    let side = |lines: &Range<u64>, bytes: &Range<u64>| {
        span(lines) > SECTION_MAX_LINES || span(bytes) > SECTION_MAX_BYTES
    };
    if side(&detail.pair.left, &detail.left_bytes) || side(&detail.pair.right, &detail.right_bytes)
    {
        return Some(StepReason::SectionCap);
    }
    let tokens = u64::try_from(DIFF_TOKEN_LIMIT).unwrap_or(u64::MAX);
    (detail.lines() > tokens).then_some(StepReason::TokenCap)
}

/// One changed block over the whole pair; a block at line 0 starts at byte 0, as Whole's fallback.
pub(crate) fn coarse_hunk(detail: &Detail) -> Hunk {
    let from_zero = |lines: &Range<u64>, bytes: &Range<u64>| {
        if lines.start == 0 {
            0..bytes.end
        } else {
            bytes.clone()
        }
    };
    Hunk {
        left_lines: to_usize(&detail.pair.left),
        right_lines: to_usize(&detail.pair.right),
        left_bytes: from_zero(&detail.pair.left, &detail.left_bytes),
        right_bytes: from_zero(&detail.pair.right, &detail.right_bytes),
    }
}

/// Diffs line slices of one pair; `None` when Histogram's token cap ran.
pub(crate) fn diff_slices(
    context: &Context,
    pair: &SectionPair,
    left: LineSet<'_>,
    right: LineSet<'_>,
) -> Result<Option<Vec<Hunk>>, Reason> {
    let alignment = diff_lines(left, right, &context.options);
    if alignment.fallback.is_some() {
        return Ok(None);
    }
    let left_shift = usize::try_from(pair.left.start).map_err(|_| Reason::Unsupported)?;
    let right_shift = usize::try_from(pair.right.start).map_err(|_| Reason::Unsupported)?;
    let mut hunks = alignment.hunks;
    for hunk in &mut hunks {
        hunk.left_lines = hunk.left_lines.start + left_shift..hunk.left_lines.end + left_shift;
        hunk.right_lines = hunk.right_lines.start + right_shift..hunk.right_lines.end + right_shift;
    }
    Ok(Some(hunks))
}

/// Diffs one pair, re-anchoring it densely when it is over a cap or the
/// budget, and leaves a coarse block only when that cannot fit.
pub(crate) fn ladder(env: &Env<'_>, pair: SectionPair, out: &mut Out) -> Result<(), Reason> {
    let mut stack = vec![(pair, 0_usize)];
    let mut held = env.limits.ledger.empty();
    while let Some((pair, depth)) = stack.pop() {
        if (env.stop)() {
            return Err(Reason::Stopped);
        }
        let forced = env.begin(&pair);
        let detail = detail(env.context, &pair, env.stop)?;
        let over = if forced {
            out.coarse(&detail, StepReason::Watchdog)?;
            None
        } else {
            attempt(env, &detail, out)?
        };
        if let Some(reason) = over {
            match split(env, &detail, depth)? {
                Some((parts, next)) => {
                    stack.extend(parts.into_iter().rev().map(|part| (part, next)))
                }
                None => out.coarse(&detail, reason)?,
            }
        }
        held.settle(bytes_of::<(SectionPair, usize)>(stack.capacity()));
        env.beat.end_section();
    }
    Ok(())
}

fn split(
    env: &Env<'_>,
    detail: &Detail,
    depth: usize,
) -> Result<Option<(Vec<SectionPair>, usize)>, Reason> {
    let rates = super::sections::RATES;
    for (at, &rate) in rates.iter().enumerate().skip(depth) {
        if let Some(parts) = refine(env, detail, rate)? {
            return Ok(Some((parts, at + 1)));
        }
    }
    Ok(None)
}

pub(crate) struct Hunks {
    pub(crate) hunks: Vec<Hunk>,
    pub(crate) digests: HunkDigests,
    pub(crate) charge: Charge,
}

pub(crate) enum ReadDiff {
    Hunks(Hunks),
    TokenCap,
    Budget,
}

pub(crate) struct Read<'a, 'b> {
    pub(crate) context: &'a Context,
    pub(crate) detail: &'a Detail,
    pub(crate) charge: &'b mut Charge,
    pub(crate) stop: Stop<'b>,
    pub(crate) before_hash: Option<&'a Hook>,
}

fn attempt(env: &Env<'_>, detail: &Detail, out: &mut Out) -> Result<Option<StepReason>, Reason> {
    if let Some(reason) = over_cap(detail) {
        return Ok(Some(reason));
    }
    let Some(mut charge) = env
        .limits
        .ledger
        .reserve(work_estimate(detail.lines(), detail.bytes()))
    else {
        return Ok(Some(StepReason::Budget));
    };
    if let Some(stall) = &env.limits.stall {
        stall(detail.pair.left.start);
    }
    let read = Read {
        context: env.context,
        detail,
        charge: &mut charge,
        stop: env.stop,
        before_hash: env.limits.before_hash.as_ref(),
    };
    match read_diff(read)? {
        ReadDiff::Hunks(hunks) => out.section(env.limits, detail, hunks)?,
        ReadDiff::TokenCap => out.coarse(detail, StepReason::TokenCap)?,
        ReadDiff::Budget => out.coarse(detail, StepReason::Budget)?,
    }
    Ok(None)
}

fn read_diff(read: Read<'_, '_>) -> Result<ReadDiff, Reason> {
    let Read {
        context,
        detail,
        charge,
        stop,
        before_hash,
    } = read;
    let pair = &detail.pair;
    let mut left = read_lines(&context.left, &pair.left, stop)?;
    let mut right = read_lines(&context.right, &pair.right, stop)?;
    let measured = lines_bytes(&left).saturating_add(lines_bytes(&right));
    charge.cover_lines(detail.lines(), detail.bytes(), measured);
    let left_set = left.set(context.left.layout.text.bom && pair.left.start == 0);
    let right_set = right.set(context.right.layout.text.bom && pair.right.start == 0);
    let Some(hunks) = diff_slices(context, pair, left_set, right_set)? else {
        return Ok(ReadDiff::TokenCap);
    };
    if let Some(before_hash) = before_hash {
        before_hash();
    }
    let Some((digests, digest_charge)) = hash_hunks(&left, &right, &hunks, charge)? else {
        return Ok(ReadDiff::Budget);
    };
    let raw = raw_bytes(&left).saturating_add(raw_bytes(&right));
    left.bytes = Vec::new();
    right.bytes = Vec::new();
    drop(charge.split(raw));
    Ok(ReadDiff::Hunks(Hunks {
        hunks,
        digests,
        charge: digest_charge,
    }))
}

/// Diffs a stored pair again under the same caps; over them it is one coarse block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rediff {
    Hunks {
        hunks: Vec<Hunk>,
        digests: HunkDigests,
    },
    Coarse {
        hunk: Hunk,
        reason: StepReason,
    },
}

pub(crate) struct Ceiling<'a> {
    pub(crate) cancel: &'a CancelToken,
    pub(crate) watchdog: std::time::Duration,
}

pub(crate) fn rediff(
    context: &Context,
    pair: &SectionPair,
    ledger: &Arc<Ledger>,
    ceiling: Ceiling<'_>,
) -> Result<(Rediff, Charge), Reason> {
    let cancel = || ceiling.cancel.is_cancelled();
    let cancel: Stop<'_> = &cancel;
    let detail = detail(context, pair, cancel)?;
    let coarse = |reason| {
        Ok((
            Rediff::Coarse {
                hunk: coarse_hunk(&detail),
                reason,
            },
            ledger.empty(),
        ))
    };
    if let Some(reason) = over_cap(&detail) {
        return coarse(reason);
    }
    let Some(mut charge) = ledger.reserve(work_estimate(detail.lines(), detail.bytes())) else {
        return coarse(StepReason::Budget);
    };
    let started = std::time::Instant::now();
    let stop = || ceiling.cancel.is_cancelled() || started.elapsed() >= ceiling.watchdog;
    let stop: Stop<'_> = &stop;
    let read = Read {
        context,
        detail: &detail,
        charge: &mut charge,
        stop,
        before_hash: None,
    };
    match read_diff(read) {
        Ok(ReadDiff::Hunks(hunks)) if started.elapsed() < ceiling.watchdog => {
            let Hunks {
                hunks,
                digests,
                charge: digest_charge,
            } = hunks;
            charge.absorb(digest_charge);
            charge.settle(
                bytes_of::<Hunk>(hunks.capacity()).saturating_add(digests.allocated_bytes()),
            );
            Ok((Rediff::Hunks { hunks, digests }, charge))
        }
        Ok(ReadDiff::Hunks(_)) if ceiling.cancel.is_cancelled() => Err(Reason::Stopped),
        Ok(ReadDiff::Hunks(_)) => coarse(StepReason::Watchdog),
        Ok(ReadDiff::TokenCap) => coarse(StepReason::TokenCap),
        Ok(ReadDiff::Budget) => coarse(StepReason::Budget),
        Err(Reason::Stopped) if ceiling.cancel.is_cancelled() => Err(Reason::Stopped),
        Err(Reason::Stopped) => coarse(StepReason::Watchdog),
        Err(reason) => Err(reason),
    }
}

pub(crate) fn coarse_batch(env: &Env<'_>, batch: &Batch) -> Result<Option<(Out, usize)>, Reason> {
    let list = &batch.sections.list;
    let Some(mut out) = Out::with_blocks(env.limits, list.len()) else {
        return coarse_one(env, batch);
    };
    for pair in list {
        env.beat.start_section(pair);
        let detail = detail(env.context, pair, env.stop);
        env.beat.end_section();
        out.coarse(&detail?, StepReason::Watchdog)?;
    }
    Ok(Some((out, list.len())))
}

fn coarse_one(env: &Env<'_>, batch: &Batch) -> Result<Option<(Out, usize)>, Reason> {
    let Some(mut out) = Out::with_blocks(env.limits, 1) else {
        return Ok(None);
    };
    let pair = SectionPair {
        left: batch.left(),
        right: batch.right(),
    };
    env.beat.start_section(&pair);
    let detail = detail(env.context, &pair, env.stop);
    env.beat.end_section();
    out.coarse(&detail?, StepReason::Watchdog)?;
    Ok(Some((out, batch.sections.list.len())))
}

pub(crate) fn read_lines(
    side: &SideData,
    lines: &Range<u64>,
    stop: Stop<'_>,
) -> Result<Lines, Reason> {
    if stop() {
        return Err(Reason::Stopped);
    }
    let at = if lines.start >= side.lines {
        side.len
    } else {
        start_of(side, lines.start, stop)?
    };
    if lines.is_empty() {
        return Ok(Lines {
            lines: Vec::new(),
            starts: vec![at],
            bytes: Vec::new(),
            base: at,
        });
    }
    unchanged(side)?;
    let until = Until {
        offset: at,
        lines: span(lines),
        end: end_of(side, lines.end, stop)?,
        len: side.len,
    };
    let read = collect(side.source.as_ref(), side.layout, until, stop)?;
    if u64::try_from(read.lines.len()).ok() != Some(span(lines)) {
        return Err(Reason::ChangedOnDisk);
    }
    unchanged(side)?;
    Ok(read)
}

fn side_span(side: &SideData, lines: &Range<u64>, stop: Stop<'_>) -> Result<Range<u64>, Reason> {
    if lines.start >= side.lines {
        return Ok(side.len..side.len);
    }
    let start = start_of(side, lines.start, stop)?;
    let end = if lines.end >= side.lines {
        side.len
    } else {
        start_of(side, lines.end, stop)?
    };
    if end < start || end > side.len {
        return Err(Reason::ChangedOnDisk);
    }
    Ok(start..end)
}

fn to_usize(range: &Range<u64>) -> Range<usize> {
    usize::try_from(range.start).unwrap_or(usize::MAX)
        ..usize::try_from(range.end).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests;
