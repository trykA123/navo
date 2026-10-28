use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::{Arc, mpsc::Sender};

use rusty_core::find::{FileHits, FindQuery};
use rusty_core::{Entry, Event, Listing, Location, PaneId, TabId};
use rusty_fs::find::{FindError, FindProgress, FoundBatch, walk_files};
use rusty_fs::{CancelToken, JobKind, JobSlot, OwnerGuard, OwnerId, Priority, volume_of};

use super::filter::Full;
use super::{Browser, Message, Waker, offline};

pub struct FindResults {
    pub query: FindQuery,
    pub hits: HashMap<String, FileHits>,
    pub progress: FindProgress,
    pub error: Option<FindError>,
    owner: Option<OwnerGuard>,
    leaving: HashMap<u64, Vec<Location>>,
}

impl FindResults {
    #[must_use]
    pub fn running(&self) -> bool {
        self.owner.is_some()
    }
}

pub(super) type FindWalk = Arc<
    dyn Fn(
            &Location,
            &FindQuery,
            &CancelToken,
            &mut dyn FnMut(FoundBatch) -> ControlFlow<()>,
        ) -> Result<FindProgress, FindError>
        + Send
        + Sync,
>;

pub(super) enum Update {
    Batch(FoundBatch),
    Done(Result<FindProgress, FindError>),
}

struct Delivery {
    tab: TabId,
    owner: OwnerId,
    sender: Sender<Message>,
    wake: Waker,
}

impl Delivery {
    fn send(&self, update: Update) -> ControlFlow<()> {
        let result = self
            .sender
            .send(Message::Found(self.tab, self.owner, update));
        (self.wake)();
        if result.is_ok() {
            ControlFlow::Continue(())
        } else {
            ControlFlow::Break(())
        }
    }
}

pub(super) fn is_results(location: &Location) -> bool {
    location.provider().as_str() == "find"
}

pub(super) fn walker() -> FindWalk {
    Arc::new(|base, query, cancel, sink| walk_files(base, query, cancel, sink))
}

impl Browser {
    #[cfg(test)]
    pub(crate) fn set_find_walk(&mut self, walk: FindWalk) {
        self.find_walk = walk;
    }

    pub(super) fn list(&mut self, tab: TabId, location: Location, request: u64) {
        if self.keep_find_listing(tab, &location, request) {
            return;
        }
        self.start_size_visit(tab, &location);
        if let Some(progress) = self.adopt_prefetch(tab, &location, request) {
            self.stalls.begin(tab, request, progress);
            self.in_flight += 1;
            return;
        }
        self.stalls
            .begin(tab, request, offline::Progress::default());
        self.submit(tab, location, request);
    }

    fn keep_find_listing(&mut self, tab: TabId, location: &Location, request: u64) -> bool {
        if !is_results(location) {
            return false;
        }
        let listing = self
            .state
            .tab(tab)
            .and_then(|tab| tab.listing().cloned())
            .unwrap_or_else(|| Listing::new(Vec::new()));
        rusty_core::apply_event(
            &mut self.state,
            Event::ListingLoaded {
                tab,
                location: location.clone(),
                request,
                listing,
            },
        );
        true
    }

    pub fn start_find(&mut self, pane: PaneId, query: FindQuery, base: Location) -> TabId {
        #[expect(
            clippy::expect_used,
            reason = "callers target a pane in the current application state"
        )]
        let tab = self
            .state
            .open_results_tab(pane, base)
            .expect("find targets an open pane");
        self.finds.insert(
            tab,
            FindResults {
                query,
                hits: HashMap::new(),
                progress: FindProgress::default(),
                error: None,
                owner: None,
                leaving: HashMap::new(),
            },
        );
        self.rerun_find(tab);
        self.run(Vec::new());
        tab
    }

    #[must_use]
    pub fn find_results(&self, tab: TabId) -> Option<&FindResults> {
        self.finds.get(&tab)
    }

    pub fn stop_find(&mut self, tab: TabId) {
        if let Some(results) = self.finds.get_mut(&tab) {
            results.owner = None;
        }
    }

    pub fn rerun_find(&mut self, tab: TabId) {
        let Some(shown) = self.state.tab(tab).filter(|tab| is_results(tab.location())) else {
            return;
        };
        let (base, location) = (shown.base_or_location().clone(), shown.location().clone());
        let Some(results) = self.finds.get_mut(&tab) else {
            return;
        };
        results.owner = None;
        results.hits.clear();
        results.progress = FindProgress::default();
        results.error = None;
        self.errors.remove(&tab);
        self.full.remove(&tab);
        self.details.insert(tab, Vec::new().into());
        apply_empty(&mut self.state, tab, location);
        let queue = self.pools.queue(JobKind::Find);
        let owner = queue.owner();
        let slot = JobSlot {
            volume: volume_of(&base),
            priority: Priority::Visible,
            owner: owner.id(),
        };
        results.owner = Some(owner);
        self.submit_find(tab, base, slot);
    }

    fn submit_find(&self, tab: TabId, base: Location, slot: JobSlot) {
        let Some(results) = self.finds.get(&tab) else {
            return;
        };
        let query = results.query.clone();
        let delivery = Delivery {
            tab,
            owner: slot.owner,
            sender: self.sender.clone(),
            wake: Arc::clone(&self.wake),
        };
        let walk = Arc::clone(&self.find_walk);
        self.pools.queue(JobKind::Find).submit(
            slot,
            Box::new(move |cancel| {
                let result = walk(&base, &query, cancel, &mut |batch| {
                    delivery.send(Update::Batch(batch))
                });
                let _ = delivery.send(Update::Done(result));
            }),
        );
    }

    pub(super) fn retain_finds(&mut self) {
        self.finds.retain(|tab, _| {
            self.state
                .tab(*tab)
                .is_some_and(|tab| tab.location().as_str() == format!("find://{}", tab.id().get()))
        });
    }

    pub(super) fn accept_find(&mut self, tab: TabId, owner: OwnerId, update: Update) -> bool {
        let Some(results) = self.finds.get_mut(&tab) else {
            return false;
        };
        if results.owner.as_ref().map(OwnerGuard::id) != Some(owner) {
            return false;
        }
        match update {
            Update::Batch(mut batch) => {
                results.progress = batch.progress;
                for file in &mut batch.files {
                    results
                        .hits
                        .insert(file.relative.to_text(), std::mem::take(&mut file.hits));
                }
                self.append_find(tab, batch.files);
            }
            Update::Done(result) => {
                results.owner = None;
                match result {
                    Ok(progress) => results.progress = progress,
                    Err(error) => results.error = Some(error),
                }
            }
        }
        true
    }

    fn append_find(&mut self, tab: TabId, files: Vec<rusty_fs::find::FoundFile>) {
        let Some(location) = self.state.tab(tab).map(|tab| tab.location().clone()) else {
            return;
        };
        let full = self.full.get(&tab);
        let mut entries = full.map_or_else(Vec::new, |full| full.listing.entries().to_vec());
        let mut details = full.map_or_else(Vec::new, |full| full.details.to_vec());
        for mut file in files {
            entries.push(Entry {
                name: file.relative.to_text(),
                is_folder: false,
            });
            file.entry.name = file.relative;
            details.push(file.entry);
        }
        let full = Full {
            listing: Listing::new(entries),
            details: details.into(),
        };
        self.show_full(tab, location, full);
    }

    fn show_full(&mut self, tab: TabId, location: Location, full: Full) {
        let (listing, details) = self.narrow(tab, &location, full);
        self.details.insert(tab, details);
        self.event(Event::ListingFiltered {
            tab,
            location,
            listing,
        });
    }
}

#[cfg(test)]
impl Browser {
    pub(crate) fn find_only(&mut self, relative: &'static [&'static str]) {
        use rusty_core::ProviderPath;
        use rusty_fs::find::FoundFile;
        use rusty_fs::{Attributes, EntryKind, FsEntry};
        self.find_walk = Arc::new(move |_, _, _, sink| {
            let files = relative
                .iter()
                .map(|name| FoundFile {
                    relative: ProviderPath::from_text(name),
                    entry: FsEntry {
                        name: ProviderPath::from_text(name),
                        kind: EntryKind::File,
                        size: 1,
                        modified_ms: None,
                        attributes: Attributes::default(),
                    },
                    hits: FileHits::default(),
                })
                .collect();
            let _ = sink(FoundBatch {
                files,
                progress: FindProgress::default(),
            });
            Ok(FindProgress::default())
        });
    }
}

fn apply_empty(state: &mut rusty_core::AppState, tab: TabId, location: Location) {
    rusty_core::apply_event(
        state,
        Event::ListingFiltered {
            tab,
            location,
            listing: Listing::new(Vec::new()),
        },
    );
}

mod leave;

#[cfg(test)]
mod tests;
