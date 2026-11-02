use std::sync::Arc;

use navo_core::paging::{ByteSource, TextLayout};

use crate::anchor::{self, SectionPair};
use crate::model::Reason;
use crate::text::scan::{self, BUCKETS, Candidate, Sample, ScanResult};

use super::super::Forge;
use super::super::batch::{Env, SideData, Stop};
use super::work::Detail;
use super::{Charge, Ledger, bytes_of, span};

pub(super) const RATES: [u32; 2] = [BUCKETS / 8, 1];
const ANCHOR_BYTES: u64 = 128;
const REFINE_BYTES: u64 = 160;
const MIN_REFINE: u64 = 64;
const INDEX_SHARE: u64 = 32;
const REFINE_MAX: u64 = 16_384;
/// Plan list, its batch copy, the store entry and its detailed slot.
pub(super) const PLAN_SECTION_BYTES: u64 = 160;

/// Sections tiling both sides; the charge covers them, their batches and their store entries.
pub(crate) struct Planned {
    pub(crate) sections: Vec<SectionPair>,
    pub(crate) terminal: bool,
    pub(crate) charge: Charge,
}

/// Anchors both sides; a sample too large for the budget is thinned by whole
/// keys, so section boundaries stay exact and only grow longer.
pub(crate) fn plan_sections(
    left: &mut ScanResult,
    right: &mut ScanResult,
    forge: Option<&Forge>,
    ledger: &Arc<Ledger>,
) -> Planned {
    if let Some(forge) = forge {
        forge(&mut left.candidates, &mut right.candidates);
    }
    let mut charge = fit(left, right, ledger);
    let left_keys = keyed(&left.candidates);
    let right_keys = keyed(&right.candidates);
    let anchors = anchor::anchors(&left_keys, &right_keys);
    drop((left_keys, right_keys));
    let terminal = anchors.last().is_some_and(|last| {
        last.left.saturating_add(1) == left.lines && last.right.saturating_add(1) == right.lines
    });
    let sections = anchor::sections(&anchors, left.lines, right.lines);
    drop(anchors);
    let entries = u64::try_from(sections.capacity()).unwrap_or(u64::MAX);
    charge.settle(entries.saturating_mul(PLAN_SECTION_BYTES));
    Planned {
        sections,
        terminal,
        charge,
    }
}

fn fit(left: &mut ScanResult, right: &mut ScanResult, ledger: &Arc<Ledger>) -> Charge {
    let mut rate = u64::from(BUCKETS);
    loop {
        let count = left.candidates.len().saturating_add(right.candidates.len());
        let sections = left.candidates.len().min(right.candidates.len()) + 1;
        let need = bytes_of::<u8>(count)
            .saturating_mul(ANCHOR_BYTES)
            .saturating_add(bytes_of::<u8>(sections).saturating_mul(PLAN_SECTION_BYTES));
        if let Some(charge) = ledger.reserve(need) {
            return charge;
        }
        if count == 0 {
            let mut charge = ledger.empty();
            charge.settle(need);
            return charge;
        }
        rate = rate.saturating_mul(2);
        for side in [&mut *left, &mut *right] {
            side.candidates
                .retain(|kept| kept.hash.is_multiple_of(rate));
        }
    }
}

/// Re-anchors one pair from a denser sample that fits the budget; `None` when it cannot.
pub(crate) fn refine(
    env: &Env<'_>,
    detail: &Detail,
    rate: u32,
) -> Result<Option<Vec<SectionPair>>, Reason> {
    let lines = span(&detail.pair.left).max(span(&detail.pair.right));
    let index = lines / INDEX_SHARE;
    let room = env.limits.ledger.available().saturating_sub(index * 2) / 2 / REFINE_BYTES;
    let cap = room.min(lines).min(REFINE_MAX);
    if cap < MIN_REFINE.min(lines.max(1)) {
        return Ok(None);
    }
    let Some(_charge) = env
        .limits
        .ledger
        .reserve(cap * 2 * REFINE_BYTES + index * 2)
    else {
        return Ok(None);
    };
    let sample = Sample {
        rate,
        cap: usize::try_from(cap).unwrap_or(usize::MAX),
    };
    let left = sample_side(
        env,
        (&env.context.left, &detail.pair.left, &detail.left_bytes),
        sample,
    )?;
    let right = sample_side(
        env,
        (&env.context.right, &detail.pair.right, &detail.right_bytes),
        sample,
    )?;
    Ok(anchor::refine(&detail.pair, &left, &right))
}

type SideSpan<'a> = (
    &'a SideData,
    &'a std::ops::Range<u64>,
    &'a std::ops::Range<u64>,
);

fn sample_side(
    env: &Env<'_>,
    (data, lines, bytes): SideSpan<'_>,
    sample: Sample,
) -> Result<Vec<anchor::Candidate<u64>>, Reason> {
    let source = Section {
        source: data.source.as_ref(),
        base: bytes.start,
        len: span(bytes),
        layout: without_bom(data.layout),
        stop: env.stop,
    };
    let mut progress = |_: u64| {};
    let scanned = scan::scan(
        &source,
        source.layout,
        &env.context.options,
        sample,
        &env.limits.cancel,
        &mut progress,
    )?;
    Ok(scanned
        .candidates
        .into_iter()
        .filter_map(|candidate| {
            let position = lines.start.checked_add(u64::from(candidate.line))?;
            lines.contains(&position).then_some(anchor::Candidate {
                key: candidate.hash,
                position,
            })
        })
        .collect())
}

fn keyed(candidates: &[Candidate]) -> Vec<anchor::Candidate<u64>> {
    candidates
        .iter()
        .map(|candidate| anchor::Candidate {
            key: candidate.hash,
            position: u64::from(candidate.line),
        })
        .collect()
}

fn without_bom(layout: TextLayout) -> TextLayout {
    let mut text = layout.text;
    text.bom = false;
    TextLayout { text }
}

struct Section<'a> {
    source: &'a dyn ByteSource,
    base: u64,
    len: u64,
    layout: TextLayout,
    stop: Stop<'a>,
}

impl ByteSource for Section<'_> {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        if (self.stop)() {
            return Err("stopped".to_owned());
        }
        if offset >= self.len {
            return Ok(0);
        }
        let want = usize::try_from((self.len - offset).min(buf.len() as u64)).unwrap_or(0);
        self.source.read_at(self.base + offset, &mut buf[..want])
    }

    fn layout(&self) -> TextLayout {
        self.layout
    }
}
