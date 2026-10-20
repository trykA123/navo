use rusty_automotive::map::{MemoryRegion, OutputSection, Symbol};

use super::facts::{Inspection, Part};
use super::text::{EMPTY, address, percent, span};
use crate::format;

pub(super) enum Row {
    Pair(&'static str, String),
    Item {
        name: String,
        detail: String,
        bytes: u64,
    },
    Note(String),
}

pub(super) struct Block {
    pub title: &'static str,
    pub rows: Vec<Row>,
}

pub(super) fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn optional(text: Option<&str>) -> String {
    text.map_or_else(|| EMPTY.to_owned(), str::to_owned)
}

fn bytes_text(size: Option<u64>) -> String {
    size.map_or_else(
        || EMPTY.to_owned(),
        |size| format!("{} B", format::bytes(size)),
    )
}

fn pair(label: &'static str, value: String) -> Row {
    Row::Pair(label, value)
}

fn item(part: &Part) -> Row {
    Row::Item {
        name: file_name(&part.object).to_owned(),
        detail: part.input.clone(),
        bytes: part.size,
    }
}

fn region_blocks(region: &MemoryRegion, objects: &[(String, u64)]) -> Vec<Block> {
    let used = format!(
        "{} · {}",
        bytes_text(Some(region.used)),
        percent(region.used, region.length)
    );
    let origin = region.origin.map(|origin| pair("Origin", address(origin)));
    let mut facts: Vec<Row> = origin.into_iter().collect();
    facts.extend([
        pair("Length", bytes_text(Some(region.length))),
        pair("Used", used),
        pair("Free", bytes_text(Some(region.free))),
    ]);
    let top = objects
        .iter()
        .map(|(name, bytes)| Row::Item {
            name: file_name(name).to_owned(),
            detail: String::new(),
            bytes: *bytes,
        })
        .collect();
    vec![
        Block {
            title: "REGION",
            rows: facts,
        },
        Block {
            title: "TOP OBJECTS",
            rows: top,
        },
    ]
}

fn section_blocks(section: &OutputSection, (parts, more): (&[Part], usize)) -> Vec<Block> {
    let facts = vec![
        pair("Range", span(section.address, section.size)),
        pair("Size", bytes_text(Some(section.size))),
        pair("Region", optional(section.region.as_deref())),
        pair("Space", optional(section.address_space.as_deref())),
    ];
    let mut rows: Vec<Row> = parts.iter().map(item).collect();
    if more > 0 {
        rows.push(Row::Note(format!("and {more} more")));
    }
    vec![
        Block {
            title: "SECTION",
            rows: facts,
        },
        Block {
            title: "CONTRIBUTIONS",
            rows,
        },
    ]
}

fn symbol_blocks(
    symbol: &Symbol,
    home: (Option<&str>, Option<&Part>),
    went_to: Option<u64>,
) -> Vec<Block> {
    let (section, held_by) = home;
    let mut facts = vec![
        pair("Address", address(symbol.address)),
        pair("Size", bytes_text(symbol.size)),
        pair("Space", optional(symbol.address_space.as_deref())),
        pair("Section", optional(section)),
        pair("Object", optional(held_by.map(|part| part.object.as_str()))),
        pair("Input", optional(held_by.map(|part| part.input.as_str()))),
    ];
    facts.extend(went_to.map(|at| pair("Went to", address(at))));
    vec![Block {
        title: "SYMBOL",
        rows: facts,
    }]
}

pub(super) fn blocks(inspection: &Inspection, went_to: Option<u64>) -> Vec<Block> {
    match inspection {
        Inspection::Region { region, objects } => region_blocks(region, objects),
        Inspection::Section {
            section,
            parts,
            more,
        } => section_blocks(section, (parts, *more)),
        Inspection::Symbol {
            symbol,
            section,
            held_by,
        } => symbol_blocks(symbol, (section.as_deref(), held_by.as_ref()), went_to),
    }
}

pub(super) fn spoken(blocks: &[Block]) -> String {
    let rows = blocks.iter().flat_map(|block| &block.rows);
    rows.map(|row| match row {
        Row::Pair(label, value) => format!("{label} {value}"),
        Row::Item {
            name,
            detail,
            bytes,
        } if detail.is_empty() => {
            format!("{name} {}", format::bytes(*bytes))
        }
        Row::Item {
            name,
            detail,
            bytes,
        } => format!("{name} {detail} {}", format::bytes(*bytes)),
        Row::Note(note) => note.clone(),
    })
    .collect::<Vec<_>>()
    .join(" ")
}
