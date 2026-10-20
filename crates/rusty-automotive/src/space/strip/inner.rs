use super::cells::{Span, merged};
use super::{Cell, Scale};
use crate::space::{Block, Owners};

const MIN_BLOCK_PX: f64 = 64.0;

#[must_use]
pub fn inner(owners: &Owners, block: &Block, scale: &Scale, width_px: u32) -> Vec<Cell> {
    let end = block.start.saturating_add(block.size);
    let wide = (scale.x(end) - scale.x(block.start)) * f64::from(width_px) > MIN_BLOCK_PX;
    let first = owners
        .items
        .partition_point(|owner| owner.start < block.start);
    let last = owners.items.partition_point(|owner| owner.start < end);
    if !wide || last.saturating_sub(first) < 2 {
        return Vec::new();
    }
    let spans = owners.items[first..last]
        .iter()
        .enumerate()
        .map(|(at, owner)| Span {
            at: first + at,
            range: owner.start..owner.start.saturating_add(owner.size).min(end),
            kind: owner.kind,
        });
    merged(spans, scale, width_px)
}
