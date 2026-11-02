mod squarify;

pub const MAX_GROUPS: usize = 22;
pub const MAX_INNER: usize = 14;
pub const INNER_MIN_WIDTH: f64 = 150.0;
pub const INNER_MIN_HEIGHT: f64 = 110.0;
pub const TEXT_MIN_WIDTH: f64 = 64.0;
pub const TEXT_MIN_HEIGHT: f64 = 34.0;
pub const HEADER_PX: f64 = 18.0;
pub const KIND_BAR_PX: f64 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    #[must_use]
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    #[must_use]
    pub fn area(&self) -> f64 {
        self.w.max(0.0) * self.h.max(0.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    pub id: u32,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: u32,
    pub value: u64,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Item(u32),
    More { count: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    pub target: Target,
    pub value: u64,
    pub rect: Rect,
    pub text: bool,
    pub inner: Vec<Tile>,
}

struct Entry<'a> {
    target: Target,
    value: u64,
    parts: &'a [Part],
}

#[must_use]
pub fn layout(items: &[Item], bounds: Rect) -> Vec<Tile> {
    let entries = cut(
        items
            .iter()
            .map(|item| (item.id, item.value, item.parts.as_slice())),
        MAX_GROUPS,
    );
    tiles(&entries, bounds, true)
}

fn cut<'a>(source: impl Iterator<Item = (u32, u64, &'a [Part])>, named: usize) -> Vec<Entry<'a>> {
    let mut live: Vec<(u32, u64, &[Part])> = source.filter(|entry| entry.1 > 0).collect();
    live.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    let rest = live.split_off(named.min(live.len()));
    let mut entries: Vec<Entry> = live
        .into_iter()
        .map(|(id, value, parts)| Entry {
            target: Target::Item(id),
            value,
            parts,
        })
        .collect();
    if !rest.is_empty() {
        entries.push(Entry {
            target: Target::More { count: rest.len() },
            value: rest
                .iter()
                .fold(0, |sum, entry| sum.saturating_add(entry.1)),
            parts: &[],
        });
    }
    entries
}

fn tiles(entries: &[Entry], bounds: Rect, nest: bool) -> Vec<Tile> {
    let values: Vec<u64> = entries.iter().map(|entry| entry.value).collect();
    squarify::squarify(&values, bounds)
        .into_iter()
        .zip(entries)
        .map(|(rect, entry)| Tile {
            target: entry.target,
            value: entry.value,
            rect,
            text: rect.w > TEXT_MIN_WIDTH && rect.h > TEXT_MIN_HEIGHT,
            inner: if nest { inner(entry, rect) } else { Vec::new() },
        })
        .collect()
}

fn inner(entry: &Entry, rect: Rect) -> Vec<Tile> {
    if entry.parts.is_empty() || rect.w <= INNER_MIN_WIDTH || rect.h <= INNER_MIN_HEIGHT {
        return Vec::new();
    }
    let area = Rect::new(
        rect.x,
        rect.y + HEADER_PX,
        rect.w,
        rect.h - HEADER_PX - KIND_BAR_PX,
    );
    let parts = cut(
        entry
            .parts
            .iter()
            .map(|part| (part.id, part.value, &[][..])),
        MAX_INNER,
    );
    tiles(&parts, area, false)
}
