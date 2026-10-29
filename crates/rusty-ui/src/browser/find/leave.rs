use std::collections::HashSet;
use std::sync::Arc;

use rusty_core::{FsErrorKind, Listing, Location, OpKind, Tab, TabId};
use rusty_fs::{JobKind, JobSlot, Priority, volume_of};

use super::super::filter::Full;
use super::super::{Browser, Message};

impl Browser {
    pub(in crate::browser) fn note_leaving(
        &mut self,
        (id, kind): (u64, OpKind),
        sources: &[Location],
        tab: TabId,
    ) {
        if !matches!(kind, OpKind::Move | OpKind::Recycle | OpKind::Delete) {
            return;
        }
        if let Some(results) = self.finds.get_mut(&tab) {
            results.leaving.insert(id, sources.to_vec());
        }
    }

    pub(in crate::browser) fn check_left(&mut self, id: u64) {
        let found = self
            .finds
            .iter_mut()
            .find_map(|(tab, results)| Some((*tab, results.leaving.remove(&id)?)));
        let Some((tab, sources)) = found else {
            return;
        };
        let Some(volume) = sources.first().map(volume_of) else {
            return;
        };
        let (provider, sender, wake) = (
            Arc::clone(&self.provider),
            self.sender.clone(),
            Arc::clone(&self.wake),
        );
        let slot = JobSlot {
            volume,
            priority: Priority::Visible,
            owner: self.owner.id(),
        };
        self.in_flight += 1;
        self.pools.queue(JobKind::Listing).submit(
            slot,
            Box::new(move |_| {
                let gone = sources
                    .into_iter()
                    .filter(|source| {
                        provider
                            .metadata(source)
                            .is_err_and(|error| error.kind == FsErrorKind::NotFound)
                    })
                    .collect();
                let _ = sender.send(Message::Left(tab, gone));
                wake();
            }),
        );
    }

    pub(in crate::browser) fn accept_left(&mut self, tab: TabId, gone: &[Location]) -> bool {
        let Some(shown) = self
            .state
            .tab(tab)
            .filter(|t| super::is_results(t.location()))
        else {
            return false;
        };
        let Some(full) = self.full.get(&tab) else {
            return false;
        };
        let stays = rows_staying(shown, &full.listing, gone);
        if stays.iter().all(|stay| *stay) {
            return false;
        }
        let mut entries = Vec::with_capacity(stays.len());
        let mut details = Vec::with_capacity(stays.len());
        let rows = full.listing.entries().iter().zip(full.details.iter());
        for ((entry, detail), stay) in rows.zip(&stays) {
            if *stay {
                entries.push(entry.clone());
                details.push(detail.clone());
            } else if let Some(results) = self.finds.get_mut(&tab) {
                let total = results
                    .hits
                    .remove(&entry.name)
                    .map_or(0, |hits| hits.total);
                results.progress.matched = results.progress.matched.saturating_sub(1);
                results.progress.hits = results.progress.hits.saturating_sub(u64::from(total));
            }
        }
        let location = shown.location().clone();
        let full = Full {
            listing: Listing::new(entries),
            details: details.into(),
        };
        self.show_full(tab, location, full);
        true
    }
}

fn rows_staying(tab: &Tab, listing: &Listing, gone: &[Location]) -> Vec<bool> {
    let gone: HashSet<&str> = gone.iter().map(Location::as_str).collect();
    listing
        .entries()
        .iter()
        .map(|entry| {
            !tab.entry_location(&entry.name)
                .is_ok_and(|place| gone.contains(place.as_str()))
        })
        .collect()
}
