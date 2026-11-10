use std::cmp::Ordering;
use std::ops::Range;

use crate::{CancelToken, EntryKind, FsEntry};

use super::natural::{NaturalArena, NaturalRef, natural_cmp};
use super::{SortBy, SortKey};

const CHECK_EVERY: usize = 4_096;

/// Shared buffers for all rows: lowercase names and their natural-order keys.
#[derive(Default)]
struct Arena {
    text: String,
    natural: NaturalArena,
}

struct Keyed {
    is_file: bool,
    extension: Range<usize>,
    natural: NaturalRef,
    size: u64,
    modified_ms: Option<i64>,
    index: u32,
}

fn keyed(entry: &FsEntry, index: u32, arena: &mut Arena) -> Keyed {
    let lower = entry.name.display().to_lowercase();
    let is_file = entry.kind != EntryKind::Folder;
    let extension = if is_file {
        lower
            .rfind('.')
            .filter(|dot| *dot > 0)
            .map_or(lower.len(), |dot| dot + 1)
    } else {
        lower.len()
    };
    let base = arena.text.len();
    arena.text.push_str(&lower);
    Keyed {
        is_file,
        extension: base + extension..base + lower.len(),
        natural: arena.natural.add(&lower),
        size: entry.size,
        modified_ms: entry.modified_ms,
        index,
    }
}

pub fn sort_order(
    rows: &[FsEntry],
    key: SortKey,
    folders_first: bool,
    cancel: &CancelToken,
) -> Option<Vec<u32>> {
    let _span = tracing::debug_span!("listing.sort").entered();
    if key.by == SortBy::Size || key.by == SortBy::Date {
        return primary_order(rows, key, folders_first, cancel);
    }
    let mut keys = Vec::with_capacity(rows.len());
    let mut arena = Arena::default();
    for (at, entry) in rows.iter().enumerate() {
        if at % CHECK_EVERY == 0 && cancel.is_cancelled() {
            return None;
        }
        keys.push(keyed(entry, u32::try_from(at).ok()?, &mut arena));
    }
    keys.sort_unstable_by(|a, b| compare(a, b, &arena, (key, folders_first)));
    if cancel.is_cancelled() {
        return None;
    }
    Some(keys.into_iter().map(|row| row.index).collect())
}

fn primary_order(
    rows: &[FsEntry],
    key: SortKey,
    folders_first: bool,
    cancel: &CancelToken,
) -> Option<Vec<u32>> {
    let mut order = Vec::with_capacity(rows.len());
    for at in 0..rows.len() {
        if at % CHECK_EVERY == 0 && cancel.is_cancelled() {
            return None;
        }
        order.push(u32::try_from(at).ok()?);
    }
    order.sort_unstable_by(|&a, &b| primary(rows, (a, b), key, folders_first));
    if cancel.is_cancelled() {
        return None;
    }
    let mut start = 0;
    while start < order.len() {
        let first = &rows[order[start] as usize];
        let mut end = start + 1;
        while end < order.len() {
            let entry = &rows[order[end] as usize];
            if !same_group(first, entry, folders_first)
                || primary_key(first, key.by) != primary_key(entry, key.by)
            {
                break;
            }
            end += 1;
        }
        if end - start > 1 {
            if cancel.is_cancelled() {
                return None;
            }
            let mut tied: Vec<(String, u32)> = order[start..end]
                .iter()
                .map(|&index| {
                    let entry = &rows[index as usize];
                    (entry.name.display().to_lowercase(), index)
                })
                .collect();
            tied.sort_by(|a, b| natural_cmp(&a.0, &b.0).then_with(|| a.1.cmp(&b.1)));
            for (slot, (_, index)) in order[start..end].iter_mut().zip(tied) {
                *slot = index;
            }
        }
        start = end;
    }
    Some(order)
}

fn primary(rows: &[FsEntry], (a, b): (u32, u32), key: SortKey, folders_first: bool) -> Ordering {
    let (a, b) = (&rows[a as usize], &rows[b as usize]);
    let groups = if folders_first {
        is_file(a).cmp(&is_file(b))
    } else {
        Ordering::Equal
    };
    let by = primary_key(a, key.by).cmp(&primary_key(b, key.by));
    let by = if key.descending { by.reverse() } else { by };
    groups.then(by)
}

fn same_group(a: &FsEntry, b: &FsEntry, folders_first: bool) -> bool {
    !folders_first || is_file(a) == is_file(b)
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Primary {
    Size(u64),
    Date(Option<i64>),
}

fn primary_key(entry: &FsEntry, by: SortBy) -> Primary {
    match by {
        SortBy::Size => Primary::Size(entry.size),
        SortBy::Date => Primary::Date(entry.modified_ms),
        SortBy::Name | SortBy::Ext => Primary::Size(0),
    }
}

fn is_file(entry: &FsEntry) -> bool {
    entry.kind != EntryKind::Folder
}

fn compare(a: &Keyed, b: &Keyed, arena: &Arena, (key, folders_first): (SortKey, bool)) -> Ordering {
    let groups = if folders_first {
        a.is_file.cmp(&b.is_file)
    } else {
        Ordering::Equal
    };
    groups.then_with(|| {
        let by_key = by(a, b, arena, key.by);
        let by_key = if key.descending {
            by_key.reverse()
        } else {
            by_key
        };
        by_key
            .then_with(|| arena.natural.compare(&a.natural, &b.natural))
            .then_with(|| a.index.cmp(&b.index))
    })
}

fn by(a: &Keyed, b: &Keyed, arena: &Arena, sort: SortBy) -> Ordering {
    match sort {
        SortBy::Name => arena.natural.compare(&a.natural, &b.natural),
        SortBy::Ext => arena.text[a.extension.clone()]
            .cmp(&arena.text[b.extension.clone()])
            .then_with(|| arena.natural.compare(&a.natural, &b.natural)),
        SortBy::Size => a.size.cmp(&b.size),
        SortBy::Date => a.modified_ms.cmp(&b.modified_ms),
    }
}

#[cfg(test)]
mod tests;
