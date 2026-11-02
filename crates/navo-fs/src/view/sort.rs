use std::cmp::Ordering;

use crate::{CancelToken, EntryKind, FsEntry};

use super::{SortBy, SortKey};

const CHECK_EVERY: usize = 4_096;

struct Keyed {
    is_file: bool,
    lower: String,
    extension: String,
    size: u64,
    modified_ms: Option<i64>,
    index: u32,
}

fn keyed(entry: &FsEntry, index: u32) -> Keyed {
    let lower = entry.name.display().to_lowercase();
    let is_file = entry.kind != EntryKind::Folder;
    let extension = match lower.rsplit_once('.') {
        Some((stem, ext)) if is_file && !stem.is_empty() => ext.to_owned(),
        _ => String::new(),
    };
    Keyed {
        is_file,
        lower,
        extension,
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
    let mut keys = Vec::with_capacity(rows.len());
    for (at, entry) in rows.iter().enumerate() {
        if at % CHECK_EVERY == 0 && cancel.is_cancelled() {
            return None;
        }
        keys.push(keyed(entry, u32::try_from(at).ok()?));
    }
    keys.sort_by(|a, b| compare(a, b, key, folders_first));
    if cancel.is_cancelled() {
        return None;
    }
    Some(keys.into_iter().map(|row| row.index).collect())
}

fn compare(a: &Keyed, b: &Keyed, key: SortKey, folders_first: bool) -> Ordering {
    let groups = if folders_first {
        a.is_file.cmp(&b.is_file)
    } else {
        Ordering::Equal
    };
    groups.then_with(|| {
        let by_key = by(a, b, key.by);
        let by_key = if key.descending {
            by_key.reverse()
        } else {
            by_key
        };
        by_key
            .then_with(|| a.lower.cmp(&b.lower))
            .then_with(|| a.index.cmp(&b.index))
    })
}

fn by(a: &Keyed, b: &Keyed, sort: SortBy) -> Ordering {
    match sort {
        SortBy::Name => a.lower.cmp(&b.lower),
        SortBy::Ext => a
            .extension
            .cmp(&b.extension)
            .then_with(|| a.lower.cmp(&b.lower)),
        SortBy::Size => a.size.cmp(&b.size),
        SortBy::Date => a.modified_ms.cmp(&b.modified_ms),
    }
}

#[cfg(test)]
mod tests;
