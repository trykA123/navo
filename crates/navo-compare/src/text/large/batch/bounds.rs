use crate::anchor::SectionPair;
use crate::model::Reason;

use super::super::budget::{
    Detail, Out, StepReason, bytes_of, detail, ladder, lines_bytes, read_lines, work_estimate,
};
use super::{Batch, Counts, Env, Outcome, coarse_fallback, same_key};

/// Runs a batch too large for one window: anchors are read line by line and
/// each pair climbs the budget ladder on its own.
pub(super) fn run_sections(env: &Env<'_>, batch: &Batch) -> Result<Outcome, Reason> {
    let list = &batch.sections.list;
    let (Some(first), Some(last)) = (list.first(), list.last()) else {
        return Ok(Outcome::Covered {
            out: Out::new(env.limits.ledger.empty()),
            counts: Counts::default(),
        });
    };
    let start = batch.start_anchor && !same_before(env, first)?;
    let end = batch.end_anchor && !same_last(env, last)?;
    if start || end {
        return Ok(Outcome::Collision { start, end });
    }
    let terminal = batch.terminal && !same_last(env, last)?;
    let mut copy = env.limits.ledger.empty();
    if !copy.try_add(bytes_of::<SectionPair>(list.len())) {
        return coarse_fallback(env, batch, StepReason::Budget);
    }
    let (sections, dropped) = merge_sections(env, list)?;
    copy.settle(bytes_of::<SectionPair>(sections.capacity()));
    let Some(mut out) = Out::with_blocks(env.limits, sections.len()) else {
        return coarse_fallback(env, batch, StepReason::Budget);
    };
    for pair in &sections {
        ladder(env, pair.clone(), &mut out)?;
    }
    let counts = Counts {
        sections: sections.len(),
        dropped: dropped + usize::from(terminal),
    };
    Ok(Outcome::Covered { out, counts })
}

fn merge_sections(
    env: &Env<'_>,
    sections: &[SectionPair],
) -> Result<(Vec<SectionPair>, usize), Reason> {
    let mut merged = Vec::with_capacity(sections.len());
    let mut rest = sections.iter();
    let Some(mut current) = rest.next().cloned() else {
        return Ok((merged, 0));
    };
    for next in rest {
        if same_last(env, &current)? {
            merged.push(std::mem::replace(&mut current, next.clone()));
        } else {
            current.left.end = next.left.end;
            current.right.end = next.right.end;
        }
    }
    merged.push(current);
    let dropped = sections.len() - merged.len();
    Ok((merged, dropped))
}

fn same_before(env: &Env<'_>, section: &SectionPair) -> Result<bool, Reason> {
    match (
        section.left.start.checked_sub(1),
        section.right.start.checked_sub(1),
    ) {
        (Some(left), Some(right)) => same_at(env, (left, right)),
        _ => Ok(true),
    }
}

fn same_last(env: &Env<'_>, section: &SectionPair) -> Result<bool, Reason> {
    match (
        section.left.end.checked_sub(1),
        section.right.end.checked_sub(1),
    ) {
        (Some(left), Some(right)) => same_at(env, (left, right)),
        _ => Ok(true),
    }
}

fn same_at(env: &Env<'_>, (left, right): (u64, u64)) -> Result<bool, Reason> {
    if left >= env.context.left.lines || right >= env.context.right.lines {
        return Ok(true);
    }
    let pair = SectionPair {
        left: left..left + 1,
        right: right..right + 1,
    };
    env.beat.start_section(&pair);
    let result = same_line(env, &pair);
    env.beat.end_section();
    result
}

fn same_line(env: &Env<'_>, pair: &SectionPair) -> Result<bool, Reason> {
    let Detail {
        left_bytes,
        right_bytes,
        ..
    } = detail(env.context, pair, env.stop)?;
    let bytes = left_bytes.end - left_bytes.start + right_bytes.end - right_bytes.start;
    let Some(mut charge) = env.limits.ledger.reserve(work_estimate(2, bytes)) else {
        return Ok(false);
    };
    let context = env.context;
    let left = read_lines(&context.left, &pair.left, env.stop)?;
    let right = read_lines(&context.right, &pair.right, env.stop)?;
    charge.cover_lines(
        2,
        bytes,
        lines_bytes(&left).saturating_add(lines_bytes(&right)),
    );
    match (left.lines.first(), right.lines.first()) {
        (Some(left), Some(right)) => Ok(same_key(left, right, &context.options)),
        _ => Ok(true),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    use std::time::Duration;

    use navo_core::paging::{ByteSource, LineIndex, TextLayout};

    use crate::anchor::SectionPair;
    use crate::model::{Budget, CancelToken};

    use super::super::super::budget::{Ledger, Limits};
    use super::super::super::schedule::Beat;
    use super::super::budget::Out;
    use super::super::{Batch, Context, Env, Outcome, Sections, SideData};
    use super::{run_sections, same_at};

    struct Huge {
        len: u64,
        reads: AtomicUsize,
    }

    impl ByteSource for Huge {
        fn len(&self) -> u64 {
            self.len
        }

        fn read_at(&self, _: u64, buf: &mut [u8]) -> Result<usize, String> {
            self.reads.fetch_add(buf.len(), Ordering::SeqCst);
            buf.fill(b'x');
            Ok(buf.len())
        }
    }

    fn setup(len: u64, budget: Budget, lines: u64) -> (Arc<Huge>, Arc<Ledger>, Context, Limits) {
        let source = Arc::new(Huge {
            len,
            reads: AtomicUsize::new(0),
        });
        let ledger = Arc::new(Ledger::new(budget));
        let side = || SideData {
            source: Arc::clone(&source) as Arc<dyn ByteSource + Send + Sync>,
            layout: TextLayout::default(),
            len,
            lines,
            index: LineIndex::new(),
        };
        let context = Context {
            left: side(),
            right: side(),
            options: Default::default(),
            _charge: ledger.empty(),
        };
        let limits = Limits {
            ledger: Arc::clone(&ledger),
            focus: Arc::new(AtomicU64::new(0)),
            paging: Arc::new(AtomicBool::new(false)),
            cancel: CancelToken::new(),
            stall: None,
            expired: None,
            watchdog: Duration::from_secs(1),
            reach: 0,
        };
        (source, ledger, context, limits)
    }

    #[test]
    fn a_huge_anchor_is_refused_before_it_is_read() {
        let len = 70 * 1024 * 1024;
        let (source, _ledger, context, limits) = setup(len, Budget::from_bytes(1024 * 1024), 1);
        let stop = || false;
        let beat = Beat::default();
        let forced = Vec::new();
        let env = Env {
            context: &context,
            limits: &limits,
            stop: &stop,
            beat: &beat,
            forced: &forced,
        };
        assert!(!same_at(&env, (0, 0)).unwrap());
        assert_eq!(source.reads.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_batch_that_cannot_reserve_its_copy_falls_back_to_one_block() {
        let budget = Out::block_bytes();
        let (_, ledger, context, limits) = setup(4, Budget::from_bytes(budget), 2);
        let list = (0..8)
            .map(|at| SectionPair {
                left: at..at + 1,
                right: at..at + 1,
            })
            .collect();
        let sections = Sections {
            list,
            charge: ledger.empty(),
        };
        let batch = Batch {
            sections: Arc::new(sections),
            start_anchor: false,
            end_anchor: false,
            terminal: false,
        };
        let beat = Beat::default();
        let forced = Vec::new();
        let stop = || false;
        let env = Env {
            context: &context,
            limits: &limits,
            stop: &stop,
            beat: &beat,
            forced: &forced,
        };
        let Outcome::Covered { out, counts } = run_sections(&env, &batch).unwrap() else {
            panic!("the one-block fallback must cover the batch");
        };
        assert_eq!(counts.sections, 8);
        assert_eq!(out.parts.len(), 1);
        assert!(ledger.peak() <= budget, "the fallback stayed in budget");
    }
}
