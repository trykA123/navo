use rusty_automotive::map::{MemoryRegion, OutputSection, Symbol};

use super::loaded::Loaded;
use super::select::Table;

const MAX_PARTS: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Part {
    pub object: String,
    pub input: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Inspection {
    Region {
        region: MemoryRegion,
        objects: Vec<(String, u64)>,
    },
    Section {
        section: OutputSection,
        parts: Vec<Part>,
        more: usize,
    },
    Symbol {
        symbol: Symbol,
        section: Option<String>,
        held_by: Option<Part>,
    },
}

fn holds(start: u64, size: u64, address: u64) -> bool {
    address >= start && address - start < size.max(1)
}

fn section_parts(loaded: &Loaded, section: &OutputSection) -> (Vec<Part>, usize) {
    let mut parts: Vec<Part> = loaded
        .model
        .contributions
        .iter()
        .filter(|part| {
            part.output_section == section.name
                && part
                    .address
                    .is_none_or(|at| holds(section.address, section.size, at))
        })
        .map(|part| Part {
            object: part.object_file.clone(),
            input: part.section.clone(),
            size: part.size,
        })
        .collect();
    parts.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.object.cmp(&b.object)));
    let more = parts.len().saturating_sub(MAX_PARTS);
    parts.truncate(MAX_PARTS);
    (parts, more)
}

fn symbol_home(loaded: &Loaded, symbol: &Symbol) -> (Option<String>, Option<Part>) {
    let section = loaded.model.sections.iter().find(|section| {
        section.address_space == symbol.address_space
            && holds(section.address, section.size, symbol.address)
    });
    let Some(section) = section else {
        return (symbol.section.clone(), None);
    };
    let held_by = loaded.model.contributions.iter().find(|part| {
        part.output_section == section.name
            && part
                .address
                .is_some_and(|at| holds(at, part.size, symbol.address))
    });
    let held_by = held_by.map(|part| Part {
        object: part.object_file.clone(),
        input: part.section.clone(),
        size: part.size,
    });
    (Some(section.name.clone()), held_by)
}

pub(super) fn inspect(loaded: &Loaded, table: Table, row: usize) -> Option<Inspection> {
    let model = &loaded.model;
    Some(match table {
        Table::Memory => {
            let region = model.regions.get(row)?;
            Inspection::Region {
                objects: loaded
                    .top_objects
                    .get(&region.name)
                    .cloned()
                    .unwrap_or_default(),
                region: region.clone(),
            }
        }
        Table::Sections => {
            let section = model.sections.get(row)?;
            let (parts, more) = section_parts(loaded, section);
            Inspection::Section {
                section: section.clone(),
                parts,
                more,
            }
        }
        Table::Symbols => {
            let symbol = model.symbols.get(row)?;
            let (section, held_by) = symbol_home(loaded, symbol);
            Inspection::Symbol {
                symbol: symbol.clone(),
                section,
                held_by,
            }
        }
    })
}
