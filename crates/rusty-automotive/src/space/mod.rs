mod kind;
mod regions;
mod table;

pub use kind::Kind;
pub use regions::{regions_for_elf, regions_for_map};

use crate::mdf::ChipKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionSource {
    Mdf,
    Map,
    Clusters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerRef {
    ElfSection(usize),
    MapSection(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub start: u64,
    pub size: u64,
    pub kind: Kind,
    pub owner: OwnerRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    pub name: String,
    pub start: Option<u64>,
    pub start_estimated: bool,
    pub total: Option<u64>,
    pub chip: Option<ChipKind>,
    pub used: u64,
    pub by_kind: [u64; Kind::COUNT],
    pub blocks: Vec<Block>,
    pub source: RegionSource,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Regions {
    pub items: Vec<Region>,
    pub outside: u64,
}

impl Region {
    #[must_use]
    pub fn free(&self) -> Option<u64> {
        self.total.map(|total| total.saturating_sub(self.used))
    }

    fn new(name: String, source: RegionSource) -> Self {
        Self {
            name,
            start: None,
            start_estimated: false,
            total: None,
            chip: None,
            used: 0,
            by_kind: [0; Kind::COUNT],
            blocks: Vec::new(),
            source,
        }
    }

    fn push(&mut self, block: Block) {
        self.used = self.used.saturating_add(block.size);
        let bytes = &mut self.by_kind[block.kind.index()];
        *bytes = bytes.saturating_add(block.size);
        self.blocks.push(block);
    }

    fn contains(&self, address: u64) -> bool {
        match (self.start, self.total) {
            (Some(start), Some(total)) => address >= start && address - start < total,
            _ => false,
        }
    }
}
