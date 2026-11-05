use std::collections::HashMap;

use navo_core::{Selection, TabId};
use navo_fs::EntryKind;
use navo_fs::view::{Rows, WeakRows};

use crate::settings::WindowParts;
use crate::status_bar::{FooterParts, StatusBarProps};

pub(super) fn footer_parts(parts: WindowParts) -> FooterParts {
    FooterParts {
        free_space: parts.free_space,
        marked_totals: parts.marked_totals,
        contents: parts.contents,
    }
}

#[derive(Clone, Copy, Default)]
struct Totals {
    folders: u64,
    files: u64,
    bytes: u64,
}

#[derive(Default)]
pub(super) struct TotalsCache {
    tabs: HashMap<TabId, (WeakRows, Totals)>,
    #[cfg(test)]
    computed: u32,
}

impl TotalsCache {
    pub(super) fn status_of(
        &mut self,
        tab: TabId,
        rows: &Rows,
        marked: &Selection,
    ) -> StatusBarProps {
        let _span = tracing::debug_span!("status.totals").entered();
        let weak = rows.downgrade();
        let totals = match self.tabs.get(&tab) {
            Some((cached, totals)) if cached.is_view_of(rows) => *totals,
            _ => {
                let totals = totals_of(rows);
                #[cfg(test)]
                {
                    self.computed += 1;
                }
                self.tabs.insert(tab, (weak, totals));
                totals
            }
        };
        let mut status = StatusBarProps {
            folders: totals.folders,
            files: totals.files,
            bytes: totals.bytes,
            ..StatusBarProps::default()
        };
        for row in marked.marked_rows() {
            let Some(entry) = usize::try_from(row).ok().and_then(|i| rows.get(i)) else {
                continue;
            };
            status.marked += 1;
            status.marked_bytes += entry.size;
        }
        status
    }
}

fn totals_of(rows: &Rows) -> Totals {
    let mut totals = Totals::default();
    for entry in rows.iter() {
        let is_folder = entry.kind == EntryKind::Folder;
        totals.folders += u64::from(is_folder);
        totals.files += u64::from(!is_folder);
        totals.bytes += entry.size;
    }
    totals
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
