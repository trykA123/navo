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

pub fn filter_order(
    rows: &[FsEntry],
    order: &[u32],
    filter: &ViewFilter,
    cancel: &CancelToken,
) -> Option<Vec<u32>> {
    let _span = tracing::debug_span!("listing.filter").entered();
    let mut scratch = Scratch::default();
    let mut kept = Vec::with_capacity(order.len());
    for (at, &index) in order.iter().enumerate() {
        if at % CHECK_EVERY == 0 && cancel.is_cancelled() {
            return None;
        }
        let entry = rows.get(usize::try_from(index).ok()?)?;
        let name = entry.name.to_text();
        if rusty_core::brand::is_reserved_name(&name) || !is_visible(entry, filter) {
            continue;
        }
        if filter
            .needle
            .is_none_or(|needle| needle.matches(&name, &mut scratch))
        {
            kept.push(index);
        }
    }
    Some(kept)
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
