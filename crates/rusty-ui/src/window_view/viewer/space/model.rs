use std::rc::Rc;
use std::sync::Arc;

use rusty_automotive::space::{
    Group, GroupBy, Grouping, Item, Origin, Owners, Part, Region, RegionSource, TOP_GROUPS,
    available, default_grouping, flash_region, group, owners, regions_for_elf,
};
use rusty_automotive::{Elf, Lines, RegionTable};
use rusty_l10n::{t, tr};

use super::sibling::{SiblingProblem, Siblings};
use super::treemap::{TreemapGroup, TreemapPart};

const SHOWN_PARTS: usize = 14;

pub(in crate::window_view::viewer) struct RichModel {
    pub regions: Vec<Arc<Region>>,
    pub outside: u64,
    pub owners: Arc<Owners>,
    pub flash: Option<usize>,
    pub grouping: Grouping,
    pub usable: [bool; 4],
    pub groups: Vec<Group>,
    pub provenance: String,
    pub region_source: Option<String>,
}

impl RichModel {
    pub fn treemap(&self) -> (Rc<[TreemapGroup]>, Vec<Item>) {
        let shown = &self.groups[..self.groups.len().min(TOP_GROUPS)];
        let groups = shown.iter().enumerate().map(|(at, group)| TreemapGroup {
            id: at as u32,
            name: group.name.clone().into(),
            by_kind: group.by_kind,
            parts: group
                .children
                .iter()
                .take(SHOWN_PARTS)
                .enumerate()
                .map(|(id, child)| TreemapPart {
                    id: id as u32,
                    name: child.name.clone().into(),
                })
                .collect(),
        });
        let items = self.groups.iter().enumerate().map(|(at, group)| Item {
            id: at as u32,
            value: group.bytes,
            parts: group
                .children
                .iter()
                .take(SHOWN_PARTS)
                .enumerate()
                .map(|(id, child)| Part {
                    id: id as u32,
                    value: child.bytes,
                })
                .collect(),
        });
        (groups.collect(), items.collect())
    }
}

fn table(siblings: &Siblings) -> Option<RegionTable> {
    match (&siblings.mdf, &siblings.map) {
        (Some(mdf), Some(map)) => Some(RegionTable::merge(map, &mdf.regions())),
        (Some(mdf), None) => Some(mdf.regions()),
        (None, Some(map)) => Some(RegionTable::from_map(map)),
        (None, None) => None,
    }
}

pub(in crate::window_view::viewer) fn build_model(
    elf: &Elf,
    lines: Option<&Lines>,
    siblings: &Siblings,
    grouping: Option<Grouping>,
) -> RichModel {
    let map = siblings.map.as_deref();
    let regions = regions_for_elf(elf, table(siblings).as_ref());
    let found = owners(elf, lines, map);
    let flash = flash_region(&regions);
    let origin = Origin::Elf { elf, map };
    let region = flash.and_then(|at| regions.items.get(at));
    let usable = region
        .and_then(Region::span)
        .map_or([false; 4], |span| available(&found, span));
    let grouping = grouping
        .filter(|grouping| usable[*grouping as usize])
        .unwrap_or_else(|| {
            region.map_or(Grouping::Module, |region| {
                default_grouping(&found, origin, region, &[])
            })
        });
    let by = GroupBy {
        grouping,
        modules: &[],
    };
    let groups = region.map_or_else(Vec::new, |region| group(&found, origin, region, &by));
    let region_source = region_source(&regions.items, siblings);
    let provenance = provenance(region_source.as_deref(), &found, siblings);
    RichModel {
        regions: regions.items.into_iter().map(Arc::new).collect(),
        outside: regions.outside,
        owners: Arc::new(found),
        flash,
        grouping,
        usable,
        groups,
        provenance,
        region_source,
    }
}

fn region_source(regions: &[Region], siblings: &Siblings) -> Option<String> {
    match regions.first()?.source {
        RegionSource::Mdf => siblings.names.mdf.clone(),
        RegionSource::Map => siblings.names.map.clone(),
        RegionSource::Clusters => None,
    }
}

fn problem_text(problem: &SiblingProblem) -> String {
    match problem {
        SiblingProblem::TooLarge { name, .. } => t!("space-sibling-too-large", name = name.clone()),
        SiblingProblem::NotRecognised { name } => {
            t!("space-sibling-not-recognised", name = name.clone())
        }
        SiblingProblem::Unreadable { name } => t!("space-sibling-unreadable", name = name.clone()),
    }
}

pub(in crate::window_view::viewer) fn provenance(
    region_source: Option<&str>,
    owners: &Owners,
    siblings: &Siblings,
) -> String {
    let regions = match region_source {
        Some(name) => t!("space-from-regions", name = name.to_owned()),
        None => tr("space-from-clusters"),
    };
    let found = &owners.provenance;
    let owned = if !found.has_symbols {
        tr("space-from-no-symbols")
    } else if found.from_map > 0 {
        let name = siblings.names.map.clone().unwrap_or_default();
        t!("space-from-map", name = name)
    } else if found.estimated > 0 {
        t!("space-from-estimated", count = found.estimated as i64)
    } else {
        tr("space-from-symbols")
    };
    let parts = [regions, owned, tr("space-from-flags")];
    parts
        .into_iter()
        .chain(siblings.problems.iter().map(problem_text))
        .collect::<Vec<_>>()
        .join(" · ")
}

#[cfg(test)]
pub(in crate::window_view::viewer) mod tests;
