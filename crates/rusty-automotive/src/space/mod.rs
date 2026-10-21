mod elf_kinds;
mod groups;
mod images;
mod kind;
mod owners_elf;
mod regions;
mod strip;
mod table;
mod treemap;

pub use groups::{
    Child, Group, GroupBy, Grouping, OTHER, Origin, TOP_GROUPS, Top, available, default_grouping,
    flash_region, group, lit_blocks, top,
};
pub use kind::Kind;
pub use owners_elf::{NOT_ATTRIBUTED, NameRef, Owner, Owners, Provenance, owners, owners_of_map};
use owners_elf::{plain_section, unnumbered};
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
    ElfImage(usize),
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
    pub reserved: Option<u64>,
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
        let taken = self.used.saturating_add(self.reserved.unwrap_or_default());
        self.total.map(|total| total.saturating_sub(taken))
    }

    #[must_use]
    pub fn span(&self) -> Option<std::ops::Range<u64>> {
        let first = self.blocks.iter().map(|block| block.start).min();
        let last = self
            .blocks
            .iter()
            .map(|block| block.start.saturating_add(block.size))
            .max();
        let start = self.start.or(first)?;
        let declared = self.total.map(|total| start.saturating_add(total));
        let end = declared.into_iter().chain(last).max()?;
        Some(start..end)
    }

    fn new(name: String, source: RegionSource) -> Self {
        Self {
            name,
            start: None,
            start_estimated: false,
            total: None,
            chip: None,
            reserved: None,
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
