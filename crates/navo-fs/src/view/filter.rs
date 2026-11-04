use std::borrow::Cow;

use crate::{CancelToken, FsEntry};

use super::{Needle, Scratch};

const CHECK_EVERY: usize = 4_096;

pub struct ViewFilter<'a> {
    pub needle: Option<&'a Needle>,
    pub show_hidden: bool,
    pub show_system: bool,
}

fn is_visible(entry: &FsEntry, filter: &ViewFilter) -> bool {
    let is_hidden = entry.attributes.hidden || entry.attributes.system;
    let is_system = entry.attributes.system;
    (filter.show_hidden || !is_hidden) && (filter.show_system || !is_system)
}

fn name_of(entry: &FsEntry) -> Cow<'_, str> {
    match entry.name.as_str() {
        Some(text) if !text.as_bytes().contains(&0xF4) => Cow::Borrowed(text),
        _ => Cow::Owned(entry.name.to_text()),
    }
}

fn kept(entry: &FsEntry, filter: &ViewFilter, scratch: &mut Scratch) -> bool {
    let name = name_of(entry);
    if navo_core::brand::is_reserved_name(&name) || !is_visible(entry, filter) {
        return false;
    }
    filter
        .needle
        .is_none_or(|needle| needle.matches(&name, scratch))
}

pub fn filter_order(
    rows: &[FsEntry],
    order: &[u32],
    filter: &ViewFilter,
    cancel: &CancelToken,
) -> Option<Vec<u32>> {
    let _span = tracing::debug_span!("listing.filter").entered();
    let mut scratch = Scratch::default();
    let mut kept_rows = Vec::with_capacity(order.len());
    for (at, &index) in order.iter().enumerate() {
        if at % CHECK_EVERY == 0 && cancel.is_cancelled() {
            return None;
        }
        let entry = rows.get(usize::try_from(index).ok()?)?;
        if kept(entry, filter, &mut scratch) {
            kept_rows.push(index);
        }
    }
    Some(kept_rows)
}

pub fn filter_all(rows: &[FsEntry], filter: &ViewFilter, cancel: &CancelToken) -> Option<Vec<u32>> {
    let _span = tracing::debug_span!("listing.filter").entered();
    let mut scratch = Scratch::default();
    let mut kept_rows = Vec::with_capacity(rows.len());
    for (at, entry) in rows.iter().enumerate() {
        if at % CHECK_EVERY == 0 && cancel.is_cancelled() {
            return None;
        }
        if kept(entry, filter, &mut scratch) {
            kept_rows.push(u32::try_from(at).ok()?);
        }
    }
    Some(kept_rows)
}

#[must_use]
pub fn materialize(rows: &[FsEntry], order: &[u32]) -> Vec<FsEntry> {
    order
        .iter()
        .filter_map(|&index| rows.get(usize::try_from(index).ok()?).cloned())
        .collect()
}

#[cfg(test)]
mod tests;
