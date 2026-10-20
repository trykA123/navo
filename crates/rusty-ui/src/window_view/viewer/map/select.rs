use std::cmp::Ordering;
use std::sync::atomic::{AtomicBool, Ordering as Flag};

use rusty_automotive::map::{MemoryRegion, OutputSection, Symbol};

use super::loaded::Loaded;
use super::scope::{Filter, Scope};

const CHECK_EVERY: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Column {
    Name,
    Address,
    Size,
    Used,
    Free,
    Percent,
    Region,
    Space,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Head {
    pub column: Column,
    pub name: &'static str,
    pub width: usize,
}

const fn head(column: Column, name: &'static str, width: usize) -> Head {
    Head {
        column,
        name,
        width,
    }
}

const MEMORY: [Head; 6] = [
    head(Column::Name, "Region", 0),
    head(Column::Address, "Origin", 13),
    head(Column::Size, "Length", 12),
    head(Column::Used, "Used", 12),
    head(Column::Free, "Free", 12),
    head(Column::Percent, "Used %", 9),
];

const SECTIONS: [Head; 5] = [
    head(Column::Name, "Section", 0),
    head(Column::Address, "Address", 13),
    head(Column::Size, "Size", 12),
    head(Column::Region, "Region", 22),
    head(Column::Space, "Space", 22),
];

const SYMBOLS: [Head; 4] = [
    head(Column::Name, "Symbol", 0),
    head(Column::Address, "Address", 13),
    head(Column::Size, "Size", 12),
    head(Column::Space, "Space", 22),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Table {
    #[default]
    Memory,
    Sections,
    Symbols,
}

impl Table {
    pub(super) const ALL: [Self; 3] = [Self::Memory, Self::Sections, Self::Symbols];

    pub(super) fn index(self) -> usize {
        self as usize
    }

    pub(super) fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Memory => "Memory",
            Self::Sections => "Sections",
            Self::Symbols => "Symbols",
        }
    }

    pub(super) fn heads(self) -> &'static [Head] {
        match self {
            Self::Memory => &MEMORY,
            Self::Sections => &SECTIONS,
            Self::Symbols => &SYMBOLS,
        }
    }

    pub(super) fn sort_keys(self) -> [Column; 4] {
        let second = match self {
            Self::Memory => Column::Percent,
            Self::Sections => Column::Region,
            Self::Symbols => Column::Space,
        };
        [Column::Name, second, Column::Address, Column::Size]
    }

    pub(super) fn default_order(self) -> Order {
        match self {
            Self::Memory => Order {
                column: Column::Percent,
                descending: true,
            },
            Self::Sections | Self::Symbols => Order {
                column: Column::Address,
                descending: false,
            },
        }
    }

    pub(super) fn len(self, loaded: &Loaded) -> usize {
        match self {
            Self::Memory => loaded.model.regions.len(),
            Self::Sections => loaded.model.sections.len(),
            Self::Symbols => loaded.model.symbols.len(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Order {
    pub column: Column,
    pub descending: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Query {
    pub table: Table,
    pub text: String,
    pub scope: Scope,
    orders: [Order; 3],
}

impl Default for Query {
    fn default() -> Self {
        Self {
            table: Table::default(),
            text: String::new(),
            scope: Scope::All,
            orders: Table::ALL.map(Table::default_order),
        }
    }
}

impl Query {
    pub(super) fn order(&self) -> Order {
        self.orders[self.table.index()]
    }

    pub(super) fn sort_by(&mut self, column: Column) {
        let old = self.order();
        self.orders[self.table.index()] = Order {
            column,
            descending: old.column == column && !old.descending,
        };
    }

    pub(super) fn is_filtering(&self) -> bool {
        !self.text.is_empty() || self.scope != Scope::All
    }
}

pub(super) fn per_mille(used: u64, length: u64) -> Option<u64> {
    if length == 0 {
        return None;
    }
    u64::try_from(u128::from(used) * 1000 / u128::from(length)).ok()
}

fn has_text(name: &str, needle: &str) -> bool {
    needle.is_empty()
        || name
            .as_bytes()
            .windows(needle.len())
            .any(|part| part.eq_ignore_ascii_case(needle.as_bytes()))
}

fn memory_order(a: &MemoryRegion, b: &MemoryRegion, column: Column) -> Ordering {
    match column {
        Column::Name => a.name.cmp(&b.name),
        Column::Address => a.origin.cmp(&b.origin),
        Column::Size => a.length.cmp(&b.length),
        Column::Used => a.used.cmp(&b.used),
        Column::Free => a.free.cmp(&b.free),
        Column::Percent => per_mille(a.used, a.length).cmp(&per_mille(b.used, b.length)),
        Column::Region | Column::Space => Ordering::Equal,
    }
}

fn section_order(a: &OutputSection, b: &OutputSection, column: Column) -> Ordering {
    match column {
        Column::Name => a.name.cmp(&b.name),
        Column::Address => a.address.cmp(&b.address),
        Column::Size => a.size.cmp(&b.size),
        Column::Region => a.region.cmp(&b.region),
        Column::Space => a.address_space.cmp(&b.address_space),
        Column::Used | Column::Free | Column::Percent => Ordering::Equal,
    }
}

fn symbol_order(a: &Symbol, b: &Symbol, column: Column) -> Ordering {
    match column {
        Column::Name => a.name.cmp(&b.name),
        Column::Address => a.address.cmp(&b.address),
        Column::Size => a.size.cmp(&b.size),
        Column::Space => a.address_space.cmp(&b.address_space),
        _ => Ordering::Equal,
    }
}

fn keep<T>(
    items: &[T],
    (name, pass): (impl Fn(&T) -> &str, impl Fn(&T) -> bool),
    (text, cancel): (&str, &AtomicBool),
) -> Option<Vec<u32>> {
    let mut rows = Vec::new();
    for (at, item) in items.iter().enumerate() {
        if at % CHECK_EVERY == 0 && cancel.load(Flag::Relaxed) {
            return None;
        }
        if pass(item) && has_text(name(item), text) {
            rows.push(u32::try_from(at).ok()?);
        }
    }
    Some(rows)
}

fn order_rows<T>(
    rows: &mut [u32],
    items: &[T],
    (order, compare): (Order, impl Fn(&T, &T, Column) -> Ordering),
) {
    rows.sort_by(|a, b| {
        let (Some(first), Some(second)) = (items.get(*a as usize), items.get(*b as usize)) else {
            return Ordering::Equal;
        };
        let primary = compare(first, second, order.column);
        let primary = if order.descending {
            primary.reverse()
        } else {
            primary
        };
        primary.then(a.cmp(b))
    });
}

pub(super) fn select(loaded: &Loaded, query: &Query, cancel: &AtomicBool) -> Option<Vec<u32>> {
    let model = &loaded.model;
    let filter = Filter::new(model, &query.scope);
    let text = (query.text.as_str(), cancel);
    let order = query.order();
    let mut rows;
    match query.table {
        Table::Memory => {
            let pass = |region: &MemoryRegion| filter.region(region);
            rows = keep(&model.regions, (|r| r.name.as_str(), pass), text)?;
            order_rows(&mut rows, &model.regions, (order, memory_order));
        }
        Table::Sections => {
            let pass = |section: &OutputSection| filter.section(section);
            rows = keep(&model.sections, (|s| s.name.as_str(), pass), text)?;
            order_rows(&mut rows, &model.sections, (order, section_order));
        }
        Table::Symbols => {
            let pass = |symbol: &Symbol| filter.symbol(symbol);
            rows = keep(&model.symbols, (|s| s.name.as_str(), pass), text)?;
            order_rows(&mut rows, &model.symbols, (order, symbol_order));
        }
    }
    (!cancel.load(Flag::Relaxed)).then_some(rows)
}
