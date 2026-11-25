//! Visible rows of the find results window, built from the hit store.

use std::cmp::Ordering;

use super::store::{FileView, HitStore};

/// Above this many hits, `LinesShown::Auto` hides hit lines.
pub const AUTO_LINES_LIMIT: u32 = 20_000;

/// Hit lines shown under a file before its `More` row.
pub const LINES_PER_FILE: usize = 3;

/// One row of the results list; ids index the hit store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Folder(u32),
    File(u32),
    Line { file: u32, nth: u16 },
    More { file: u32, hidden: u32 },
}

/// How the rows are shaped; `sort` is the key and whether it runs descending.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowMode {
    pub grouped: bool,
    pub lines: LinesShown,
    pub sort: Option<(SortKey, bool)>,
}

/// Whether hit lines show under their file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinesShown {
    Auto,
    On,
    Off,
}

/// The column the files sort by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Hits,
    Size,
    Modified,
}

/// Builds the visible rows: discovery order unless sorted, folders first when grouped.
///
/// Ties keep discovery order. When grouped, folders sort by path under `Name` and otherwise by
/// their first file in the sorted order.
#[must_use]
pub fn rows(store: &HitStore, mode: &RowMode) -> Vec<Row> {
    let lines = match mode.lines {
        LinesShown::Auto => store.hit_total() <= AUTO_LINES_LIMIT,
        LinesShown::On => true,
        LinesShown::Off => false,
    };
    let mut files: Vec<u32> = (0..count(store.file_count())).collect();
    if let Some((key, descending)) = mode.sort {
        files.sort_by(|a, b| directed(compare_files(store, *a, *b, key), descending));
    }
    let mut out = Vec::with_capacity(files.len());
    if !mode.grouped {
        for file in files {
            push_file(&mut out, store, file, lines);
        }
        return out;
    }
    for (folder, group) in groups(store, &files, mode.sort) {
        out.push(Row::Folder(folder));
        for file in group {
            push_file(&mut out, store, file, lines);
        }
    }
    out
}

/// Position of `row` in `rows`.
#[must_use]
pub fn index_of(rows: &[Row], row: Row) -> Option<usize> {
    rows.iter().position(|candidate| *candidate == row)
}

/// Position of `row`, or of its file row when a line or `More` row vanished, else the top.
#[must_use]
pub fn nearest(rows: &[Row], row: Row) -> usize {
    index_of(rows, row)
        .or_else(|| match row {
            Row::Line { file, .. } | Row::More { file, .. } => index_of(rows, Row::File(file)),
            Row::Folder(_) | Row::File(_) => None,
        })
        .unwrap_or(0)
}

/// Splits sorted files into folders, in the order the folders appear among them.
fn groups(store: &HitStore, files: &[u32], sort: Option<(SortKey, bool)>) -> Vec<(u32, Vec<u32>)> {
    let mut slot: Vec<Option<usize>> = vec![None; store.folder_count()];
    let mut groups: Vec<(u32, Vec<u32>)> = Vec::new();
    for &file in files {
        let Some(folder) = store.file(file).map(|view| view.folder_id) else {
            continue;
        };
        let Some(entry) = slot.get_mut(folder as usize) else {
            continue;
        };
        let index = *entry.get_or_insert_with(|| {
            groups.push((folder, Vec::new()));
            groups.len() - 1
        });
        if let Some((_, group)) = groups.get_mut(index) {
            group.push(file);
        }
    }
    if let Some((SortKey::Name, descending)) = sort {
        groups.sort_by(|(a, _), (b, _)| {
            let path = |id: u32| store.folder(id).map_or("", |folder| folder.path);
            directed(compare_names(path(*a), path(*b)), descending)
        });
    }
    groups
}

/// Adds a file row and, when lines show, up to three line rows and a `More` row.
fn push_file(out: &mut Vec<Row>, store: &HitStore, file: u32, lines: bool) {
    out.push(Row::File(file));
    if !lines {
        return;
    }
    let Some(view) = store.file(file) else {
        return;
    };
    let shown = view.lines().take(LINES_PER_FILE).count();
    for nth in 0..shown {
        out.push(Row::Line {
            file,
            nth: u16::try_from(nth).unwrap_or(u16::MAX),
        });
    }
    let hidden = view.hits.saturating_sub(count(shown));
    if shown > 0 && hidden > 0 {
        out.push(Row::More { file, hidden });
    }
}

fn compare_files(store: &HitStore, a: u32, b: u32, key: SortKey) -> Ordering {
    let (Some(a), Some(b)) = (store.file(a), store.file(b)) else {
        return Ordering::Equal;
    };
    compare_views(&a, &b, key)
}

fn compare_views(a: &FileView<'_>, b: &FileView<'_>, key: SortKey) -> Ordering {
    match key {
        SortKey::Name => compare_names(a.name, b.name),
        SortKey::Hits => a.hits.cmp(&b.hits),
        SortKey::Size => a.size.cmp(&b.size),
        SortKey::Modified => a.modified_ms.cmp(&b.modified_ms),
    }
}

/// Case-insensitive name order, then exact order so case-only differences stay stable.
fn compare_names(a: &str, b: &str) -> Ordering {
    folded(a).cmp(folded(b)).then_with(|| a.cmp(b))
}

fn folded(text: &str) -> impl Iterator<Item = char> + '_ {
    text.chars().flat_map(char::to_lowercase)
}

fn directed(order: Ordering, descending: bool) -> Ordering {
    if descending { order.reverse() } else { order }
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
