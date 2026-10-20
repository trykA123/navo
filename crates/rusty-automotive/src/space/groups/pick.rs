use std::ops::Range;

use super::{GroupBy, Grouping, Origin, group};
use crate::mdf::ChipKind;
use crate::space::{Kind, Owners, Region, Regions};

#[must_use]
pub fn flash_region(regions: &Regions) -> Option<usize> {
    let rank = |region: &Region| {
        let code = region.by_kind[Kind::Code.index()] + region.by_kind[Kind::Rodata.index()];
        let is_rom = region.chip == Some(ChipKind::Rom) && code > 0;
        (is_rom, code, region.used)
    };
    let mut best: Option<(usize, (bool, u64, u64))> = None;
    for (at, region) in regions.items.iter().enumerate() {
        let key = rank(region);
        if best.is_none_or(|(_, most)| key > most) {
            best = Some((at, key));
        }
    }
    best.map(|(at, _)| at)
}

#[must_use]
pub fn available(owners: &Owners, span: Range<u64>) -> [bool; 4] {
    let items = owners.in_span(span);
    let any = |has: &dyn Fn(&crate::space::Owner) -> bool| items.iter().any(has);
    [
        !items.is_empty(),
        any(&|owner| owner.source_file.is_some()),
        any(&|owner| owner.object.is_some()),
        !items.is_empty(),
    ]
}

#[must_use]
pub fn default_grouping(
    owners: &Owners,
    origin: Origin<'_>,
    region: &Region,
    modules: &[String],
) -> Grouping {
    let Some(span) = region.span() else {
        return Grouping::Module;
    };
    let usable = available(owners, span);
    let mut first = None;
    for (grouping, usable) in Grouping::ALL.into_iter().zip(usable) {
        if !usable {
            continue;
        }
        first.get_or_insert(grouping);
        let by = GroupBy { grouping, modules };
        if group(owners, origin, region, &by).len() > 1 {
            return grouping;
        }
    }
    first.unwrap_or(Grouping::Module)
}
