use std::collections::{BTreeMap, HashMap};

use rusty_automotive::map::MapModel;

pub(super) const TOP_OBJECTS: usize = 10;

type TopObjects = HashMap<String, Vec<(String, u64)>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ObjectRow {
    pub name: String,
    pub bytes: u64,
    pub regions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RegionRow {
    pub name: String,
    pub used: u64,
    pub free: u64,
}

impl RegionRow {
    pub(super) fn length(&self) -> u64 {
        self.used.saturating_add(self.free)
    }
}

pub(super) struct Loaded {
    pub model: MapModel,
    pub regions: Vec<RegionRow>,
    pub objects: Vec<ObjectRow>,
    pub top_objects: TopObjects,
    pub spaces: Vec<Option<String>>,
    pub has_origins: bool,
}

fn merge_objects(model: &MapModel) -> (Vec<ObjectRow>, TopObjects) {
    let mut objects = BTreeMap::<&str, (u64, Vec<&str>)>::new();
    let mut per_region = TopObjects::new();
    for total in model.object_totals(None) {
        let (bytes, regions) = objects.entry(total.object_file).or_default();
        *bytes = bytes.saturating_add(total.bytes);
        if let Some(region) = total.region {
            regions.push(region);
            per_region
                .entry(region.to_owned())
                .or_default()
                .push((total.object_file.to_owned(), total.bytes));
        }
    }
    let mut objects: Vec<ObjectRow> = objects
        .into_iter()
        .map(|(name, (bytes, regions))| ObjectRow {
            name: name.to_owned(),
            bytes,
            regions: regions.into_iter().map(str::to_owned).collect(),
        })
        .collect();
    objects.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));
    for list in per_region.values_mut() {
        list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        list.truncate(TOP_OBJECTS);
    }
    (objects, per_region)
}

fn spaces(model: &MapModel) -> Vec<Option<String>> {
    let mut spaces: Vec<Option<String>> = Vec::new();
    for symbol in &model.symbols {
        if !spaces.contains(&symbol.address_space) {
            spaces.push(symbol.address_space.clone());
        }
    }
    spaces
}

impl Loaded {
    pub(super) fn new(model: MapModel) -> Self {
        let (objects, top_objects) = merge_objects(&model);
        let regions = model
            .region_totals()
            .into_iter()
            .map(|total| RegionRow {
                name: total.region.to_owned(),
                used: total.used,
                free: total.free,
            })
            .collect();
        Self {
            regions,
            objects,
            top_objects,
            spaces: spaces(&model),
            has_origins: model.regions.iter().any(|region| region.origin.is_some()),
            model,
        }
    }

    pub(super) fn used(&self) -> u64 {
        self.regions
            .iter()
            .fold(0, |sum, region| sum.saturating_add(region.used))
    }

    pub(super) fn free(&self) -> u64 {
        self.regions
            .iter()
            .fold(0, |sum, region| sum.saturating_add(region.free))
    }
}
