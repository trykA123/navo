use std::cmp::Ordering;
use std::sync::atomic::{self, AtomicBool};

use object::elf::{STT_FILE, STT_SECTION};

use super::{Bind, Elf, Place, Symbol, SymbolKind};

const CHECK_EVERY: usize = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Name,
    Address,
    Size,
    Share,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub text: String,
    pub scope: Scope,
    pub order: Order,
    pub hide_noise: bool,
}

impl Default for Query {
    fn default() -> Self {
        Self {
            text: String::new(),
            scope: Scope::default(),
            order: Order::default(),
            hide_noise: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub rows: Vec<u32>,
    pub hidden: u64,
}

fn is_estimable(symbol: &Symbol) -> bool {
    symbol.size == 0
        && matches!(symbol.bind, Bind::Global | Bind::Weak)
        && matches!(symbol.kind, SymbolKind::Func | SymbolKind::Object)
}

fn is_boundary(symbol: &Symbol) -> bool {
    symbol.size != 0 || symbol.bind != Bind::Local
}

pub(crate) fn estimate(elf: &Elf, symbol: &Symbol, next: Option<u64>) -> Option<u64> {
    if !is_estimable(symbol) {
        return None;
    }
    let span = elf.section_of(symbol)?.span();
    if !span.contains(&symbol.value) {
        return None;
    }
    Some(next.map_or(span.end, |next| next.min(span.end)) - symbol.value)
}

#[must_use]
pub fn estimated_size(elf: &Elf, at: usize) -> Option<u64> {
    let symbol = elf.symbols.get(at)?;
    let next = elf
        .symbols
        .iter()
        .filter(|other| {
            other.place == symbol.place && other.value > symbol.value && is_boundary(other)
        })
        .map(|other| other.value)
        .min();
    estimate(elf, symbol, next)
}

#[derive(Debug, Clone, Default)]
pub struct Boundaries(Vec<Vec<u64>>);

impl Boundaries {
    #[must_use]
    pub fn new(elf: &Elf) -> Self {
        let mut by_section = vec![Vec::new(); elf.sections.len()];
        for symbol in &elf.symbols {
            if is_boundary(symbol)
                && let Place::Section(index) = symbol.place
                && let Some(addresses) = by_section.get_mut(index as usize)
            {
                addresses.push(symbol.value);
            }
        }
        for addresses in &mut by_section {
            addresses.sort_unstable();
            addresses.dedup();
        }
        Self(by_section)
    }

    #[must_use]
    pub fn estimated_size(&self, elf: &Elf, symbol: &Symbol) -> Option<u64> {
        estimate(elf, symbol, self.after(symbol))
    }

    pub(crate) fn after(&self, symbol: &Symbol) -> Option<u64> {
        let Place::Section(index) = symbol.place else {
            return None;
        };
        let addresses = self.0.get(index as usize)?;
        addresses
            .get(addresses.partition_point(|address| *address <= symbol.value))
            .copied()
    }
}

fn is_noise(elf: &Elf, boundaries: &Boundaries, symbol: &Symbol) -> bool {
    symbol.place == Place::Absolute
        || matches!(symbol.kind, SymbolKind::Other(STT_FILE | STT_SECTION))
        || (symbol.size == 0 && estimate(elf, symbol, boundaries.after(symbol)).is_none())
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
        Column::Size | Column::Share => a.size.cmp(&b.size),
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

fn matches_text(name: &str, needle: &str, is_ascii: bool) -> bool {
    if is_ascii {
        contains_ignoring_case(name, needle)
    } else {
        name.to_lowercase().contains(needle)
    }
}

#[must_use]
pub fn select(elf: &Elf, query: &Query, cancel: &AtomicBool) -> Option<Selection> {
    let needle = query.text.to_lowercase();
    let is_ascii = needle.is_ascii();
    let order = query.order;
    let boundaries =
        (query.hide_noise || order.column == Column::Share).then(|| Boundaries::new(elf));
    let mut rows = Vec::new();
    let mut hidden = 0;
    for (at, symbol) in elf.symbols.iter().enumerate() {
        if at % CHECK_EVERY == 0 && cancel.load(atomic::Ordering::Relaxed) {
            return None;
        }
        if !in_scope(elf, symbol, query.scope) || !matches_text(elf.name(symbol), &needle, is_ascii)
        {
            continue;
        }
        match &boundaries {
            Some(boundaries) if query.hide_noise && is_noise(elf, boundaries, symbol) => {
                hidden += 1;
            }
            _ => rows.push((
                at as u32,
                shown_size(elf, boundaries.as_ref(), symbol, order),
            )),
        }
    }
    let symbol = |at: &u32| &elf.symbols[*at as usize];
    rows.sort_by(|(a, weight_a), (b, weight_b)| {
        let ordering = if order.column == Column::Share {
            weight_a
                .cmp(weight_b)
                .then_with(|| by_name(elf.name(symbol(a)), elf.name(symbol(b))))
        } else {
            compare(elf, order.column, (symbol(a), symbol(b)))
        };
        if order.descending {
            ordering.reverse()
        } else {
            ordering
        }
    });
    let rows = rows.into_iter().map(|(at, _)| at).collect();
    (!cancel.load(atomic::Ordering::Relaxed)).then_some(Selection { rows, hidden })
}

fn shown_size(elf: &Elf, boundaries: Option<&Boundaries>, symbol: &Symbol, order: Order) -> u64 {
    match boundaries {
        Some(boundaries) if order.column == Column::Share && symbol.size == 0 => {
            boundaries.estimated_size(elf, symbol).unwrap_or(0)
        }
        _ => symbol.size,
    }
}
