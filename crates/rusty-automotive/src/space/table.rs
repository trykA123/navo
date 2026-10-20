use crate::map::MapModel;
use crate::mdf::{Region, RegionTable, TableOrigin};

impl RegionTable {
    #[must_use]
    pub fn from_map(map: &MapModel) -> Self {
        Self::build(map, None)
    }

    #[must_use]
    pub fn merge(map: &MapModel, mdf: &Self) -> Self {
        if map.regions.is_empty() {
            return mdf.clone();
        }
        Self::build(map, Some(mdf))
    }

    fn build(map: &MapModel, mdf: Option<&Self>) -> Self {
        let mut regions = totals(map);
        for region in &mut regions {
            let chip = mdf.and_then(|table| named(table, &region.name));
            region.kind = chip.and_then(|chip| chip.kind);
            if region.start.is_none() {
                region.start = chip.and_then(|chip| chip.start);
            }
            if region.start.is_none() {
                region.start = lowest_section(map, &region.name);
                region.start_estimated = region.start.is_some();
            }
        }
        Self {
            regions,
            origin: TableOrigin::Map,
        }
    }
}

fn totals(map: &MapModel) -> Vec<Region> {
    let mut regions: Vec<Region> = Vec::new();
    for row in &map.regions {
        match regions.iter_mut().find(|region| region.name == row.name) {
            Some(region) => {
                region.size = region.size.saturating_add(row.length);
                region.start = match (region.start, row.origin) {
                    (Some(known), Some(origin)) => Some(known.min(origin)),
                    (known, origin) => known.or(origin),
                };
            }
            None => regions.push(Region {
                name: row.name.clone(),
                start: row.origin,
                start_estimated: false,
                size: row.length,
                kind: None,
            }),
        }
    }
    regions
}

fn named<'a>(table: &'a RegionTable, name: &str) -> Option<&'a Region> {
    table
        .regions
        .iter()
        .find(|region| region.name.eq_ignore_ascii_case(name))
}

fn lowest_section(map: &MapModel, region: &str) -> Option<u64> {
    map.sections
        .iter()
        .filter(|section| {
            section
                .region
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(region))
        })
        .map(|section| section.address)
        .min()
}
