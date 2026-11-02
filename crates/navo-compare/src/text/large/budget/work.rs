use std::ops::Range;
use std::sync::Arc;

use crate::anchor::SectionPair;
use crate::model::{CancelToken, Reason};
use crate::text::diff::{Hunk, diff_lines};
use crate::text::side::{DIFF_TOKEN_LIMIT, LineSet};

use super::super::batch::{Batch, Context, Env, SideData, Stop, unchanged};
use super::super::read::{Lines, Until, collect, start_of};
use super::sections::refine;
use super::{Charge, Ledger, Out, StepReason, bytes_of, lines_bytes, span, work_estimate};

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
    match read_diff(env.context, detail, &mut charge, env.stop)? {
        Some(hunks) => out.section(env.limits, detail, hunks)?,
        None => out.coarse(detail, StepReason::TokenCap)?,
    }
    Ok(None)
}

fn read_diff(
    context: &Context,
    detail: &Detail,
    charge: &mut Charge,
    stop: Stop<'_>,
) -> Result<Option<Vec<Hunk>>, Reason> {
    let pair = &detail.pair;
    let left = read_lines(&context.left, &pair.left, stop)?;
    let right = read_lines(&context.right, &pair.right, stop)?;
    let measured = lines_bytes(&left).saturating_add(lines_bytes(&right));
    charge.cover_lines(detail.lines(), detail.bytes(), measured);
    let left_set = left.set(context.left.layout.text.bom && pair.left.start == 0);
    let right_set = right.set(context.right.layout.text.bom && pair.right.start == 0);
    diff_slices(context, pair, left_set, right_set)
}

/// Diffs a stored pair again under the same caps; over them it is one coarse block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rediff {
    Hunks(Vec<Hunk>),
    Coarse { hunk: Hunk, reason: StepReason },
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
) -> Result<Rediff, Reason> {
    let cancel = || ceiling.cancel.is_cancelled();
    let cancel: Stop<'_> = &cancel;
    let detail = detail(context, pair, cancel)?;
    if let Some(reason) = over_cap(&detail) {
        return Ok(Rediff::Coarse {
            hunk: coarse_hunk(&detail),
            reason,
        });
    }
    let Some(mut charge) = ledger.reserve(work_estimate(detail.lines(), detail.bytes())) else {
        return Ok(Rediff::Coarse {
            hunk: coarse_hunk(&detail),
            reason: StepReason::Budget,
        });
    };
    let started = std::time::Instant::now();
    let stop = || ceiling.cancel.is_cancelled() || started.elapsed() >= ceiling.watchdog;
    let stop: Stop<'_> = &stop;
    match read_diff(context, &detail, &mut charge, stop) {
        Ok(Some(hunks)) if started.elapsed() < ceiling.watchdog => Ok(Rediff::Hunks(hunks)),
        Ok(Some(_)) if ceiling.cancel.is_cancelled() => Err(Reason::Stopped),
        Ok(Some(_)) => Ok(Rediff::Coarse {
            hunk: coarse_hunk(&detail),
            reason: StepReason::Watchdog,
        }),
        Err(Reason::Stopped) if ceiling.cancel.is_cancelled() => Err(Reason::Stopped),
        Err(Reason::Stopped) => Ok(Rediff::Coarse {
            hunk: coarse_hunk(&detail),
            reason: StepReason::Watchdog,
        }),
        Ok(None) => Ok(Rediff::Coarse {
            hunk: coarse_hunk(&detail),
            reason: StepReason::TokenCap,
        }),
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
        });
    }
    unchanged(side)?;
    let until = Until {
        offset: at,
        lines: span(lines),
        end: side.len,
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
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64};

    use navo_core::paging::LineIndex;

    use crate::model::{Budget, CancelToken};
    use crate::text::large::tests::side;

    use super::super::super::batch::{Batch, Context, Env, Sections, SideData};
    use super::super::{Ledger, Limits, Out, WATCHDOG};
    use super::coarse_batch;
    use crate::text::large::schedule::Beat;

    fn context(ledger: &Arc<Ledger>) -> Context {
        let large = side(b"a\nb\nc\nd\n");
        let data = || SideData {
            source: Arc::clone(&large.source),
            layout: large.layout,
            len: 8,
            lines: 4,
            index: LineIndex::new(),
        };
        Context {
            left: data(),
            right: data(),
            options: Default::default(),
            _charge: ledger.empty(),
        }
    }

    #[test]
    fn recovery_emits_one_block_when_the_ledger_is_full() {
        let budget = Out::block_bytes();
        let ledger = Arc::new(Ledger::new(Budget::from_bytes(budget)));
        let limits = Limits {
            ledger: Arc::clone(&ledger),
            focus: Arc::new(AtomicU64::new(0)),
            paging: Arc::new(AtomicBool::new(false)),
            cancel: CancelToken::new(),
            stall: None,
            expired: None,
            watchdog: WATCHDOG,
            reach: 0,
        };
        let list = (0..4)
            .map(|at| crate::anchor::SectionPair {
                left: at..at + 1,
                right: at..at + 1,
            })
            .collect();
        let batch = Batch {
            sections: Arc::new(Sections {
                list,
                charge: ledger.empty(),
            }),
            start_anchor: false,
            end_anchor: false,
            terminal: false,
        };
        let beat = Beat::default();
        let forced = Vec::new();
        let stop = || false;
        let env = Env {
            context: &context(&ledger),
            limits: &limits,
            stop: &stop,
            beat: &beat,
            forced: &forced,
        };
        let (out, sections) = coarse_batch(&env, &batch)
            .expect("recovery reads the batch")
            .expect("one fallback block fits");
        assert_eq!(sections, 4);
        assert_eq!(out.parts.len(), 1);
        assert!(ledger.peak() <= budget);
    }
}
