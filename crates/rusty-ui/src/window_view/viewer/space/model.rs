use std::rc::Rc;
use std::sync::Arc;

use rusty_automotive::space::{
    Group, GroupBy, Grouping, Item, Origin, Owners, Part, Provenance, Region, RegionSource,
    Regions, TOP_GROUPS, available, default_grouping, elf_kinds_match, flash_region, group, owners,
    owners_of_map, regions_for_elf, regions_for_map,
};
use rusty_automotive::{Boundaries, Elf, Lines, MapModel, Mdf, RegionTable};
use rusty_l10n::{t, tr};

use super::sibling::{SiblingProblem, Siblings};
use super::treemap::{TreemapGroup, TreemapPart};

const SHOWN_PARTS: usize = 14;

#[derive(Default)]
pub(in crate::window_view::viewer) struct GroupingOptions {
    pub grouping: Option<Grouping>,
    pub modules: Arc<[String]>,
}

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
    pub stripped: bool,
    pub from_map: bool,
    pub boundaries: Option<Boundaries>,
    pub modules: Arc<[String]>,
}

impl RichModel {
    pub fn outside_note(&self) -> Option<String> {
        let name = self.region_source.clone()?;
        (self.outside > 0).then(|| t!("space-outside", count = self.outside as i64, name = name))
    }

    fn set_provenance(&mut self, text: String, origin: Origin<'_>) {
        let count = self
            .flash
            .and_then(|at| self.regions.get(at))
            .filter(|_| !self.modules.is_empty())
            .map_or(0, |region| {
                let by = GroupBy {
                    grouping: Grouping::Module,
                    modules: &self.modules,
                };
                group(&self.owners, origin, region, &by)
                    .iter()
                    .filter(|group| group.listed)
                    .count()
            });
        self.provenance = if count > 0 {
            format!(
                "{text} · {}",
                t!("space-modules-listed", count = count as i64)
            )
        } else {
            text
        };
    }

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
                    by_kind: child.by_kind,
                })
                .collect(),
        });
        let items = self.groups.iter().enumerate().map(|(at, group)| Item {
            id: at as u32,
            value: group.bytes,
            parts: group
                .children
                .iter()
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

pub(super) fn table(siblings: &Siblings) -> Option<RegionTable> {
    match (&siblings.mdf, &siblings.map) {
        (Some(mdf), Some(map)) => Some(RegionTable::merge(map, &mdf.regions())),
        (Some(mdf), None) => Some(mdf.regions()),
        (None, Some(map)) => Some(RegionTable::from_map(map)),
        (None, None) => None,
    }
}

pub(super) fn assemble(
    (regions, found): (Regions, Owners),
    origin: Origin<'_>,
    options: GroupingOptions,
) -> RichModel {
    let flash = flash_region(&regions);
    let region = flash.and_then(|at| regions.items.get(at));
    let usable = region
        .and_then(Region::span)
        .map_or([false; 4], |span| available(&found, span));
    let grouping = options
        .grouping
        .filter(|grouping| usable[*grouping as usize])
        .unwrap_or_else(|| {
            region.map_or(Grouping::Module, |region| {
                default_grouping(&found, origin, region, &options.modules)
            })
        });
    let by = GroupBy {
        grouping,
        modules: &options.modules,
    };
    let groups = region.map_or_else(Vec::new, |region| group(&found, origin, region, &by));
    RichModel {
        regions: regions.items.into_iter().map(Arc::new).collect(),
        outside: regions.outside,
        stripped: !found.provenance.has_symbols,
        owners: Arc::new(found),
        flash,
        grouping,
        usable,
        groups,
        provenance: String::new(),
        region_source: None,
        from_map: matches!(origin, Origin::Map(_)),
        boundaries: None,
        modules: options.modules,
    }
}

pub(in crate::window_view::viewer) fn build_model(
    elf: &Elf,
    lines: Option<&Lines>,
    siblings: &Siblings,
    options: GroupingOptions,
) -> RichModel {
    let map = siblings.map.as_deref();
    let regions = regions_for_elf(elf, table(siblings).as_ref());
    let found = owners(elf, lines, map);
    let names = (siblings.names.mdf.as_deref(), siblings.names.map.as_deref());
    let region_source = region_source(&regions.items, names);
    let owned = elf_owners_text(&found.provenance, siblings);
    let provenance = provenance(
        region_source.as_deref(),
        [owned, tr("space-from-flags")],
        siblings,
    );
    let origin = Origin::Elf { elf, map };
    let mut model = RichModel {
        region_source,
        boundaries: Some(Boundaries::new(elf)),
        ..assemble((regions, found), origin, options)
    };
    model.set_provenance(provenance, origin);
    model
}

pub(in crate::window_view::viewer) fn build_map_model(
    map: &MapModel,
    siblings: &Siblings,
    (name, options): (&str, GroupingOptions),
) -> RichModel {
    let elf = siblings.elf.as_deref();
    let regions = regions_for_map(map, siblings.mdf.as_ref().map(Mdf::regions).as_ref(), elf);
    let found = owners_of_map(map, elf);
    let region_source = region_source(&regions.items, (siblings.names.mdf.as_deref(), Some(name)));
    let owned = if found.provenance.from_map > 0 {
        t!("space-from-map", name = name.to_owned())
    } else {
        tr("space-from-no-objects")
    };
    let kinds = match (&siblings.names.elf, elf) {
        (Some(name), Some(elf)) if elf_kinds_match(map, elf) => {
            t!("space-from-elf", name = name.clone())
        }
        _ => tr("space-from-names"),
    };
    let provenance = provenance(region_source.as_deref(), [owned, kinds], siblings);
    let origin = Origin::Map(map);
    let mut model = RichModel {
        region_source,
        stripped: false,
        ..assemble((regions, found), origin, options)
    };
    model.set_provenance(provenance, origin);
    model
}

fn elf_owners_text(found: &Provenance, siblings: &Siblings) -> String {
    if !found.has_symbols {
        tr("space-from-no-symbols")
    } else if found.from_map > 0 {
        let name = siblings.names.map.clone().unwrap_or_default();
        t!("space-from-map", name = name)
    } else if found.estimated > 0 {
        t!("space-from-estimated", count = found.estimated as i64)
    } else {
        tr("space-from-symbols")
    }
}

fn region_source(regions: &[Region], (mdf, map): (Option<&str>, Option<&str>)) -> Option<String> {
    match regions.first()?.source {
        RegionSource::Mdf => mdf,
        RegionSource::Map => map,
        RegionSource::Clusters => None,
    }
    .map(str::to_owned)
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

fn provenance(
    region_source: Option<&str>,
    [owned, kinds]: [String; 2],
    siblings: &Siblings,
) -> String {
    let regions = match region_source {
        Some(name) => region_provenance(name, siblings),
        None => tr("space-from-clusters"),
    };
    [regions, owned, kinds]
        .into_iter()
        .chain(siblings.problems.iter().map(problem_text))
        .collect::<Vec<_>>()
        .join(" · ")
}

fn region_provenance(name: &str, siblings: &Siblings) -> String {
    let names = &siblings.names;
    let extension = if names.only_mdf && names.mdf.as_deref() == Some(name) {
        Some("mdf")
    } else if names.only_map && names.map.as_deref() == Some(name) {
        Some("map")
    } else {
        None
    };
    let name = extension.map_or_else(
        || name.to_owned(),
        |extension| {
            t!(
                "space-only-companion",
                name = name.to_owned(),
                extension = extension
            )
        },
    );
    t!("space-from-regions", name = name)
}

#[cfg(test)]
pub(in crate::window_view::viewer) mod tests;
