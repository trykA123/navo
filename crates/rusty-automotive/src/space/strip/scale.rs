use std::ops::Range;

use crate::space::Region;

const COMPRESS_OVER_PERCENT: u64 = 4;
const COMPRESSED_SHARE: f64 = 0.015;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleMode {
    Compressed,
    True,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Break {
    pub start: u64,
    pub size: u64,
    pub x0: f64,
    pub x1: f64,
}

#[derive(Debug, Clone, Copy)]
struct Segment {
    start: u64,
    len: u64,
    begin: f64,
    weight: f64,
}

#[derive(Debug, Clone)]
pub struct Scale {
    domain: Range<u64>,
    segments: Vec<Segment>,
    total: f64,
    breaks: Vec<Break>,
}

impl Scale {
    #[must_use]
    pub fn new(region: &Region, mode: ScaleMode, view: Option<Range<u64>>) -> Self {
        let domain = view
            .filter(|view| view.end > view.start)
            .unwrap_or_else(|| extent(region));
        let gaps = match mode {
            ScaleMode::Compressed if region.total.is_some() => long_gaps(region, &domain),
            _ => Vec::new(),
        };
        let pieces = pieces(&domain, &gaps);
        let share = (domain.end - domain.start) as f64 * COMPRESSED_SHARE;
        let mut segments = Vec::with_capacity(pieces.len());
        let mut breaks = Vec::new();
        let mut total = 0.0;
        for (range, compressed) in pieces {
            let len = range.end - range.start;
            let weight = if compressed { share } else { len as f64 };
            segments.push(Segment {
                start: range.start,
                len,
                begin: total,
                weight,
            });
            if compressed {
                breaks.push(Break {
                    start: range.start,
                    size: len,
                    x0: total,
                    x1: total + weight,
                });
            }
            total += weight;
        }
        for item in &mut breaks {
            item.x0 /= total;
            item.x1 /= total;
        }
        Self {
            domain,
            segments,
            total,
            breaks,
        }
    }

    #[must_use]
    pub fn domain(&self) -> Range<u64> {
        self.domain.clone()
    }

    #[must_use]
    pub fn breaks(&self) -> &[Break] {
        &self.breaks
    }

    #[must_use]
    pub fn compressed_bytes(&self) -> u64 {
        self.breaks.iter().map(|item| item.size).sum()
    }

    #[must_use]
    pub fn x(&self, offset: u64) -> f64 {
        let Some(last) = self.segments.len().checked_sub(1) else {
            return 0.0;
        };
        let offset = offset.clamp(self.domain.start, self.domain.end);
        let at = self
            .segments
            .partition_point(|part| part.start + part.len <= offset)
            .min(last);
        let part = self.segments[at];
        let within = (offset - part.start) as f64 / part.len as f64;
        (part.begin + within * part.weight) / self.total
    }

    #[must_use]
    pub fn offset(&self, x: f64) -> u64 {
        let Some(last) = self.segments.len().checked_sub(1) else {
            return self.domain.start;
        };
        let target = x.clamp(0.0, 1.0) * self.total;
        let at = self
            .segments
            .partition_point(|part| part.begin + part.weight <= target)
            .min(last);
        let part = self.segments[at];
        let within = ((target - part.begin) / part.weight).clamp(0.0, 1.0);
        part.start + (within * part.len as f64).round() as u64
    }
}

fn extent(region: &Region) -> Range<u64> {
    let first = region.blocks.first().map(|block| block.start);
    let start = match (region.start, first) {
        (Some(start), Some(first)) => start.min(first),
        (start, first) => start.or(first).unwrap_or(0),
    };
    let reserved = start.saturating_add(region.total.unwrap_or(0));
    let used = region
        .blocks
        .iter()
        .map(|block| block.start.saturating_add(block.size))
        .max()
        .unwrap_or(start);
    start..reserved.max(used)
}

fn long_gaps(region: &Region, domain: &Range<u64>) -> Vec<Range<u64>> {
    let len = u128::from(domain.end - domain.start);
    let limit = (len * u128::from(COMPRESS_OVER_PERCENT) / 100) as u64;
    let mut gaps = Vec::new();
    let mut covered = domain.start;
    let mut any = false;
    for block in &region.blocks {
        let start = block.start.max(domain.start);
        let end = block.start.saturating_add(block.size).min(domain.end);
        if end <= start {
            continue;
        }
        any = true;
        if start - covered.min(start) > limit {
            gaps.push(covered..start);
        }
        covered = covered.max(end);
    }
    if any && domain.end - covered > limit {
        gaps.push(covered..domain.end);
    }
    gaps
}

fn pieces(domain: &Range<u64>, gaps: &[Range<u64>]) -> Vec<(Range<u64>, bool)> {
    let mut out = Vec::new();
    let mut at = domain.start;
    for gap in gaps {
        if gap.start > at {
            out.push((at..gap.start, false));
        }
        out.push((gap.clone(), true));
        at = gap.end;
    }
    if domain.end > at {
        out.push((at..domain.end, false));
    }
    out
}
