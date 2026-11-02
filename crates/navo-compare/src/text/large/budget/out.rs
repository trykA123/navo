use std::ops::Range;

use crate::anchor::SectionPair;
use crate::model::Reason;
use crate::text::diff::Hunk;

use super::work::{Detail, coarse_hunk};
use super::{Charge, Limits, StepInfo, StepReason, bytes_of, grow_vec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Detail,
    Paged(u64),
    Coarse,
}

/// One final section pair and the end of its hunks in `Out::hunks`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Part {
    pub(crate) pair: SectionPair,
    pub(crate) end: usize,
    pub(crate) kind: Kind,
}

/// A batch's result; its charge covers every vector's capacity.
pub(crate) struct Out {
    pub(crate) hunks: Vec<Hunk>,
    pub(crate) parts: Vec<Part>,
    pub(crate) steps: Vec<StepInfo>,
    pub(crate) charge: Charge,
    planned: usize,
}

impl Out {
    pub(crate) fn new(charge: Charge) -> Self {
        Self {
            hunks: Vec::new(),
            parts: Vec::new(),
            steps: Vec::new(),
            charge,
            planned: 0,
        }
    }

    /// Bytes of one coarse block.
    pub(crate) fn block_bytes() -> u64 {
        bytes_of::<Hunk>(1)
            .saturating_add(bytes_of::<Part>(1))
            .saturating_add(bytes_of::<StepInfo>(1))
    }

    /// Reserves one fallback block per section before any detail can spend it.
    pub(crate) fn with_blocks(limits: &Limits, blocks: usize) -> Option<Self> {
        let need = bytes_of::<Hunk>(blocks)
            .saturating_add(bytes_of::<Part>(blocks))
            .saturating_add(bytes_of::<StepInfo>(blocks));
        let charge = limits.ledger.reserve(need)?;
        let mut out = Self::new(charge);
        out.hunks.reserve_exact(blocks);
        out.parts.reserve_exact(blocks);
        out.steps.reserve_exact(blocks);
        out.planned = blocks;
        Some(out)
    }

    pub(crate) fn hunks_of(&self, index: usize) -> Range<usize> {
        let start = index
            .checked_sub(1)
            .and_then(|before| self.parts.get(before))
            .map_or(0, |part| part.end);
        start..self.parts.get(index).map_or(start, |part| part.end)
    }

    /// Keeps a section's line hunks, or its count when it is far from the view.
    pub(crate) fn section(
        &mut self,
        limits: &Limits,
        detail: &Detail,
        hunks: Vec<Hunk>,
    ) -> Result<(), Reason> {
        if !self.ensure_block() {
            return Err(Reason::Unsupported);
        }
        if limits.distant(&detail.pair.left) {
            let count = u64::try_from(hunks.len()).unwrap_or(u64::MAX);
            drop(hunks);
            return self.block(detail, Kind::Paged(count), StepReason::Paged);
        }
        if !self.reserve_detail(hunks.len()) {
            drop(hunks);
            return self.coarse(detail, StepReason::Budget);
        }
        self.hunks.extend(hunks);
        self.parts.push(Part {
            pair: detail.pair.clone(),
            end: self.hunks.len(),
            kind: Kind::Detail,
        });
        Ok(())
    }

    pub(crate) fn coarse(&mut self, detail: &Detail, reason: StepReason) -> Result<(), Reason> {
        self.block(detail, Kind::Coarse, reason)
    }

    fn ensure_block(&mut self) -> bool {
        if self.remaining() > 0 {
            return true;
        }
        if !grow_vec(&mut self.hunks, 1, &mut self.charge)
            || !grow_vec(&mut self.parts, 1, &mut self.charge)
            || !grow_vec(&mut self.steps, 1, &mut self.charge)
        {
            return false;
        }
        self.planned = self.planned.saturating_add(1);
        true
    }

    fn reserve_detail(&mut self, hunks: usize) -> bool {
        let reserve = self.remaining().saturating_sub(1);
        grow_vec(
            &mut self.hunks,
            hunks.saturating_add(reserve),
            &mut self.charge,
        ) && grow_vec(
            &mut self.parts,
            1usize.saturating_add(reserve),
            &mut self.charge,
        ) && grow_vec(&mut self.steps, reserve, &mut self.charge)
    }

    fn remaining(&self) -> usize {
        self.planned.saturating_sub(self.parts.len())
    }

    fn room(&self) -> bool {
        let reserve = self.remaining();
        self.hunks.len().saturating_add(reserve) <= self.hunks.capacity()
            && self.parts.len().saturating_add(reserve) <= self.parts.capacity()
            && self.steps.len().saturating_add(reserve) <= self.steps.capacity()
    }

    fn block(&mut self, detail: &Detail, kind: Kind, reason: StepReason) -> Result<(), Reason> {
        if !self.ensure_block() || !self.room() {
            return Err(Reason::Unsupported);
        }
        self.hunks.push(coarse_hunk(detail));
        self.parts.push(Part {
            pair: detail.pair.clone(),
            end: self.hunks.len(),
            kind,
        });
        self.steps.push(StepInfo::new(&detail.pair, reason));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64};
    use std::time::Duration;

    use crate::model::{Budget, CancelToken};

    use super::super::Ledger;
    use super::*;

    fn limits(ledger: &Arc<Ledger>) -> Limits {
        Limits {
            ledger: Arc::clone(ledger),
            focus: Arc::new(AtomicU64::new(0)),
            paging: Arc::new(AtomicBool::new(false)),
            cancel: CancelToken::new(),
            stall: None,
            expired: None,
            watchdog: Duration::from_secs(1),
            reach: 0,
        }
    }

    fn detail(line: u64) -> Detail {
        Detail {
            pair: SectionPair {
                left: line..line + 1,
                right: line..line + 1,
            },
            left_bytes: line..line + 1,
            right_bytes: line..line + 1,
        }
    }

    fn hunks(count: usize) -> Vec<Hunk> {
        let hunk = Hunk {
            left_lines: 0..0,
            right_lines: 0..0,
            left_bytes: 0..0,
            right_bytes: 0..0,
        };
        vec![hunk; count]
    }

    #[test]
    fn detail_pressure_still_emits_every_fallback_block() {
        let blocks = 8;
        let reserved = bytes_of::<Hunk>(blocks)
            .saturating_add(bytes_of::<Part>(blocks))
            .saturating_add(bytes_of::<StepInfo>(blocks));
        let ledger = Arc::new(Ledger::new(Budget::from_bytes(
            reserved.saturating_add(bytes_of::<Hunk>(3)),
        )));
        let limits = limits(&ledger);
        let mut out = Out::with_blocks(&limits, blocks).expect("fallback capacity fits");
        out.section(&limits, &detail(0), hunks(8))
            .expect("the pressured detail falls back to coarse");
        for line in 1..blocks as u64 {
            out.coarse(&detail(line), StepReason::Budget)
                .expect("a fallback block fits");
        }
        assert_eq!(out.parts.len(), blocks, "no block is silently dropped");
        assert_eq!(out.hunks.len(), blocks);
        assert_eq!(out.steps.len(), blocks);
    }

    #[test]
    fn a_fallback_reservation_that_does_not_fit_fails_loudly() {
        let ledger = Arc::new(Ledger::new(Budget::from_bytes(1)));
        let limits = limits(&ledger);
        assert!(Out::with_blocks(&limits, 4).is_none());
        assert_eq!(ledger.used(), 0);
    }
}
