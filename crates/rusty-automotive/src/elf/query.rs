use std::cmp::Ordering;
use std::sync::atomic::{self, AtomicBool};

use super::{Elf, Place, Symbol};

const CHECK_EVERY: usize = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Name,
    Address,
    Size,
    Kind,
    Bind,
    Section,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Order {
    pub column: Column,
    pub descending: bool,
}

impl Default for Order {
    fn default() -> Self {
        Self {
            column: Column::Address,
            descending: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Scope {
    #[default]
    All,
    Section(u32),
    Segment(usize),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    pub text: String,
    pub scope: Scope,
    pub order: Order,
}

fn contains_ignoring_case(name: &str, needle: &str) -> bool {
    let (name, needle) = (name.as_bytes(), needle.as_bytes());
    needle.is_empty()
        || name
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
}

fn in_scope(elf: &Elf, symbol: &Symbol, scope: Scope) -> bool {
    match scope {
        Scope::All => true,
        Scope::Section(index) => symbol.place == Place::Section(index),
        Scope::Segment(at) => {
            matches!(symbol.place, Place::Section(_))
                && elf
                    .segments
                    .get(at)
                    .is_some_and(|segment| segment.span().contains(&symbol.value))
        }
    }
}

fn section_name<'a>(elf: &'a Elf, symbol: &Symbol) -> &'a str {
    elf.section_of(symbol)
        .map_or("", |section| section.name.as_str())
}

fn compare(elf: &Elf, column: Column, (a, b): (&Symbol, &Symbol)) -> Ordering {
    let by_column = match column {
        Column::Name => Ordering::Equal,
        Column::Address => a.value.cmp(&b.value),
        Column::Size => a.size.cmp(&b.size),
        Column::Kind => a.kind.cmp(&b.kind),
        Column::Bind => a.bind.cmp(&b.bind),
        Column::Section => section_name(elf, a).cmp(section_name(elf, b)),
    };
    by_column.then_with(|| by_name(elf.name(a), elf.name(b)))
}

fn by_name(a: &str, b: &str) -> Ordering {
    let folded = a
        .bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .cmp(b.bytes().map(|byte| byte.to_ascii_lowercase()));
    folded.then_with(|| a.cmp(b))
}

#[must_use]
pub fn select(elf: &Elf, query: &Query, cancel: &AtomicBool) -> Option<Vec<u32>> {
    let needle = query.text.to_lowercase();
    let is_ascii = needle.is_ascii();
    let mut rows = Vec::new();
    for (at, symbol) in elf.symbols.iter().enumerate() {
        if at % CHECK_EVERY == 0 && cancel.load(atomic::Ordering::Relaxed) {
            return None;
        }
        if !in_scope(elf, symbol, query.scope) {
            continue;
        }
        let name = elf.name(symbol);
        let is_match = if is_ascii {
            contains_ignoring_case(name, &needle)
        } else {
            name.to_lowercase().contains(&needle)
        };
        if is_match {
            rows.push(at as u32);
        }
    }
    let order = query.order;
    let symbol = |at: &u32| &elf.symbols[*at as usize];
    rows.sort_by(|a, b| {
        let ordering = compare(elf, order.column, (symbol(a), symbol(b)));
        if order.descending {
            ordering.reverse()
        } else {
            ordering
        }
    });
    (!cancel.load(atomic::Ordering::Relaxed)).then_some(rows)
}
