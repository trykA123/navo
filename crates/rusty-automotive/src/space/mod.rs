mod kind;
mod owners_elf;
mod regions;
mod strip;
mod table;
mod treemap;

pub use kind::Kind;
pub use owners_elf::{NOT_ATTRIBUTED, NameRef, Owner, Owners, Provenance, owners};
pub use regions::{regions_for_elf, regions_for_map};
pub use strip::{Break, Cell, Scale, ScaleMode, cells, inner};
pub use treemap::{
    HEADER_PX, INNER_MIN_HEIGHT, INNER_MIN_WIDTH, Item, KIND_BAR_PX, MAX_GROUPS, MAX_INNER, Part,
    Rect, TEXT_MIN_HEIGHT, TEXT_MIN_WIDTH, Target, Tile, layout,
};

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
        if block.kind != Kind::Fill {
            self.used = self.used.saturating_add(block.size);
        }
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
