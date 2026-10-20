mod lookup;
mod module;
mod pick;
mod table;

use std::ops::Range;

use self::lookup::Lookup;
use self::module::Modules;
pub use self::pick::{available, default_grouping, flash_region};
use self::table::Table;
use super::{Kind, Owners, Region};
use crate::{Elf, MapModel};

pub const OTHER: &str = "(other)";
pub const TOP_GROUPS: usize = 22;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grouping {
    Module,
    SourceFile,
    ObjectFile,
    Section,
}

impl Grouping {
    pub const ALL: [Self; 4] = [
        Self::Module,
        Self::SourceFile,
        Self::ObjectFile,
        Self::Section,
    ];
}

#[derive(Debug, Clone, Copy)]
pub enum Origin<'a> {
    Elf {
        elf: &'a Elf,
        map: Option<&'a MapModel>,
    },
    Map(&'a MapModel),
}

#[derive(Debug, Clone, Copy)]
pub struct GroupBy<'a> {
    pub grouping: Grouping,
    pub modules: &'a [String],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Child {
    pub name: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    pub by_kind: [u64; Kind::COUNT],
    pub bytes: u64,
    pub image: u64,
    pub items: u32,
    pub listed: bool,
    pub children: Vec<Child>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Top<'a> {
    pub shown: &'a [Group],
    pub more: usize,
    pub more_bytes: u64,
}

#[must_use]
pub fn top(groups: &[Group], limit: usize) -> Top<'_> {
    let (shown, rest) = groups.split_at(limit.min(groups.len()));
    Top {
        shown,
        more: rest.len(),
        more_bytes: rest.iter().map(|group| group.bytes).sum(),
    }
}

#[must_use]
pub fn group(owners: &Owners, origin: Origin<'_>, region: &Region, by: &GroupBy<'_>) -> Vec<Group> {
    let modules = Modules::new(by.modules);
    let lookup = Lookup {
        owners,
        origin,
        modules: &modules,
        grouping: by.grouping,
    };
    let mut table = Table::default();
    for span in block_spans(region) {
        for owner in owners.in_span(span) {
            if owner.kind != Kind::Fill {
                table.add(lookup.key(owner), owner, lookup.child(owner));
            }
        }
    }
    table.finish()
}

fn block_spans(region: &Region) -> Vec<Range<u64>> {
    let mut spans: Vec<Range<u64>> = region
        .blocks
        .iter()
        .map(|block| block.start..block.start.saturating_add(block.size))
        .collect();
    spans.sort_by_key(|span| span.start);
    let mut merged: Vec<Range<u64>> = Vec::with_capacity(spans.len());
    for span in spans {
        match merged.last_mut() {
            Some(last) if span.start <= last.end => last.end = last.end.max(span.end),
            _ => merged.push(span),
        }
    }
    merged
}
