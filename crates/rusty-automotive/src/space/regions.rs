use crate::elf::Elf;
use crate::map::MapModel;
use crate::mdf::{Region as Declared, RegionTable, TableOrigin};

use super::elf_kinds::ElfKinds;
use super::images::images;
use super::kind::takes_no_address_space;
use super::{Block, Kind, OwnerRef, Region, RegionSource, Regions};

const CLUSTER_GAP: u64 = 16 * 1024 * 1024;

struct Placed<'a> {
    block: Block,
    region: Option<&'a str>,
}

#[must_use]
pub fn regions_for_elf(elf: &Elf, table: Option<&RegionTable>) -> Regions {
    let placed = elf
        .sections
        .iter()
        .enumerate()
        .filter(|(_, section)| {
            section.is_loaded() && section.size > 0 && !takes_no_address_space(section)
        })
        .map(|(at, section)| Placed {
            block: Block {
                start: section.address,
                size: section.size,
                kind: Kind::of_section(section),
                owner: OwnerRef::ElfSection(at),
            },
            region: None,
        })
        .chain(images(elf).into_iter().map(|image| Placed {
            block: Block {
                start: image.start,
                size: image.size,
                kind: Kind::Data,
                owner: OwnerRef::ElfImage(image.section),
            },
            region: None,
        }))
        .collect();
    assemble(placed, table)
}

#[must_use]
pub fn regions_for_map(map: &MapModel, mdf: Option<&RegionTable>, elf: Option<&Elf>) -> Regions {
    let table = match mdf {
        Some(mdf) => RegionTable::merge(map, mdf),
        None => RegionTable::from_map(map),
    };
    let kinds = ElfKinds::new(elf);
    let placed = map
        .sections
        .iter()
        .enumerate()
        .filter(|(_, section)| section.size > 0)
        .map(|(at, section)| Placed {
            block: Block {
                start: section.address,
                size: section.size,
                kind: kinds.of(section),
                owner: OwnerRef::MapSection(at),
            },
            region: section.region.as_deref(),
        })
        .collect();
    assemble(placed, Some(&table))
}

fn assemble(placed: Vec<Placed<'_>>, table: Option<&RegionTable>) -> Regions {
    match table.filter(|table| !table.regions.is_empty()) {
        Some(table) => declared(placed, table),
        None => clusters(placed),
    }
}

fn declared(placed: Vec<Placed<'_>>, table: &RegionTable) -> Regions {
    let source = match table.origin {
        TableOrigin::Mdf => RegionSource::Mdf,
        TableOrigin::Map => RegionSource::Map,
        TableOrigin::MapAndMdf => RegionSource::MapAndMdf,
    };
    let mut items: Vec<Region> = table
        .regions
        .iter()
        .map(|declared| open(declared, source))
        .collect();
    let mut outside = 0;
    for part in placed {
        match find(&items, &part) {
            Some(at) => items[at].push(part.block),
            None => outside += 1,
        }
    }
    for region in &mut items {
        region.blocks.sort_by_key(|block| block.start);
    }
    Regions { items, outside }
}

fn open(declared: &Declared, source: RegionSource) -> Region {
    Region {
        start: declared.start,
        start_estimated: declared.start_estimated,
        total: Some(declared.size),
        chip: declared.kind,
        reserved: declared.reserved,
        ..Region::new(declared.name.clone(), source)
    }
}

fn find(items: &[Region], part: &Placed<'_>) -> Option<usize> {
    part.region
        .and_then(|name| {
            items
                .iter()
                .position(|region| region.name.eq_ignore_ascii_case(name))
        })
        .or_else(|| {
            items
                .iter()
                .position(|region| region.contains(part.block.start))
        })
}

fn clusters(mut placed: Vec<Placed<'_>>) -> Regions {
    placed.sort_by_key(|part| part.block.start);
    let mut items: Vec<Region> = Vec::new();
    let mut end = 0;
    for part in placed {
        let block = part.block;
        if items.is_empty() || block.start.saturating_sub(end) >= CLUSTER_GAP {
            items.push(Region {
                start: Some(block.start),
                start_estimated: true,
                ..Region::new(format!("0x{:08X}", block.start), RegionSource::Clusters)
            });
            end = 0;
        }
        end = end.max(block.start.saturating_add(block.size));
        if let Some(cluster) = items.last_mut() {
            cluster.push(block);
        }
    }
    Regions { items, outside: 0 }
}
