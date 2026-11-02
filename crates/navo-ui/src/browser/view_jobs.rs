use std::collections::HashMap;
use std::sync::Arc;

use navo_core::{AppState, Location, TabId};
use navo_fs::{CancelToken, FsEntry, JobKind, JobSlot, Priority, volume_of};

use super::cached::SYNC_ROWS;
use super::filter::{Full, Shown, Visibility, shown_until};
use super::sort::sorted_full;
use super::{Browser, Message};
use crate::file_list::SortKey;

pub(super) struct ViewJob {
    revision: u64,
    cancel: CancelToken,
    location: Location,
    rows: Arc<[FsEntry]>,
    resized: Vec<(String, u64)>,
    carry: Carry,
}

struct Carry {
    sorts: bool,
    lands: bool,
    fallback: String,
}

#[derive(Default)]
pub(super) struct ViewJobs {
    next: u64,
    jobs: HashMap<TabId, ViewJob>,
}

impl ViewJobs {
    pub(super) fn cancel(&mut self, tab: TabId) -> Option<ViewJob> {
        let job = self.jobs.remove(&tab)?;
        job.cancel.cancel();
        Some(job)
    }

    pub(super) fn drop_stale(&mut self, state: &AppState) {
        self.jobs.retain(|tab, job| {
            let is_current = state
                .tab(*tab)
                .is_some_and(|t| t.location() == &job.location);
            if !is_current {
                job.cancel.cancel();
            }
            is_current
        });
    }

    pub(super) fn resized(
        &mut self,
        tab: TabId,
        (from, to): (&Arc<[FsEntry]>, &Arc<[FsEntry]>),
        sizes: &[(String, u64)],
    ) {
        let Some(job) = self.jobs.get_mut(&tab) else {
            return;
        };
        if Arc::ptr_eq(&job.rows, from) {
            job.rows = Arc::clone(to);
            job.resized.extend_from_slice(sizes);
        }
    }

    fn finish(&mut self, stamp: &Stamp) -> Option<ViewJob> {
        let job = self.jobs.get(&stamp.tab)?;
        let is_current = job.revision == stamp.revision && job.location == stamp.location;
        let tab = stamp.tab;
        is_current.then(|| self.jobs.remove(&tab)).flatten()
    }

    #[cfg(test)]
    pub(super) fn is_idle(&self) -> bool {
        self.jobs.is_empty()
    }
}

pub(super) enum ViewChange {
    Text(String),
    Sort,
    Again,
}

struct Stamp {
    tab: TabId,
    location: Location,
    revision: u64,
    shows: Visibility,
}

pub(super) struct Viewed {
    stamp: Stamp,
    text: String,
    full: Option<Full>,
    shown: Shown,
}

struct ViewWork {
    stamp: Stamp,
    base: Full,
    sort: Option<(SortKey, bool)>,
    texts: [String; 2],
}

impl ViewWork {
    fn run(self, cancel: &CancelToken) -> Option<Viewed> {
        let _span = tracing::debug_span!("listing.view_job").entered();
        let full = self
            .sort
            .map(|(key, folders_first)| sorted_full(&self.base.details, key, folders_first));
        if cancel.is_cancelled() {
            return None;
        }
        let base = full.as_ref().unwrap_or(&self.base);
        let shows = self.stamp.shows;
        let [text, fallback] = self.texts;
        let (text, shown) = [text, fallback, String::new()]
            .into_iter()
            .find_map(|text| shown_until(base, &text, shows, cancel).map(|shown| (text, shown)))?;
        (!cancel.is_cancelled()).then_some(Viewed {
            stamp: self.stamp,
            text,
            full,
            shown,
        })
    }
}

impl Browser {
    pub(super) fn views_off_thread(&self, tab: TabId) -> bool {
        self.full
            .get(&tab)
            .is_some_and(|full| full.details.len() > SYNC_ROWS)
    }

    pub(super) fn request_view(&mut self, tab: TabId, change: ViewChange) -> bool {
        let pending = self.view_jobs.cancel(tab);
        self.submit_view(tab, change, pending)
    }

    fn submit_view(&mut self, tab: TabId, change: ViewChange, pending: Option<ViewJob>) -> bool {
        let Some(location) = self
            .state
            .tab(tab)
            .filter(|t| t.listing().is_some())
            .map(|t| t.location().clone())
        else {
            return false;
        };
        let Some(base) = self.full.get(&tab).cloned() else {
            return false;
        };
        let previous = self.filter_text(tab).unwrap_or_default().to_owned();
        let carry = next_carry(&change, &previous, pending.map(|job| job.carry));
        let text = match change {
            ViewChange::Text(text) => text,
            ViewChange::Sort | ViewChange::Again => previous,
        };
        self.store_filter(tab, &location, text.clone());
        self.view_jobs.next += 1;
        let work = ViewWork {
            stamp: Stamp {
                tab,
                location: location.clone(),
                revision: self.view_jobs.next,
                shows: self.visibility(),
            },
            base,
            sort: carry
                .sorts
                .then(|| (self.sort_key(tab), self.folders_first)),
            texts: [text, carry.fallback.clone()],
        };
        let job = ViewJob {
            revision: self.view_jobs.next,
            cancel: CancelToken::new(),
            location,
            rows: Arc::clone(&work.base.details),
            resized: Vec::new(),
            carry,
        };
        self.spawn_view(work, job.cancel.clone());
        self.view_jobs.jobs.insert(tab, job);
        true
    }

    fn spawn_view(&self, work: ViewWork, cancel: CancelToken) {
        #[cfg(test)]
        counters::bump(&counters::SUBMITTED);
        let (sender, wake) = (self.sender.clone(), Arc::clone(&self.wake));
        let slot = JobSlot {
            volume: volume_of(&work.stamp.location),
            priority: Priority::Visible,
            owner: self.owner.id(),
        };
        self.pools.queue(JobKind::Listing).submit(
            slot,
            Box::new(move |pool| {
                let viewed = work.run(&cancel).filter(|_| !pool.is_cancelled());
                let Some(viewed) = viewed else {
                    return;
                };
                if sender.send(Message::Viewed(viewed)).is_err() {
                    tracing::debug!("view result dropped: browser closed");
                    return;
                }
                wake();
            }),
        );
    }

    pub(super) fn accept_viewed(&mut self, viewed: Viewed) -> bool {
        let Stamp {
            tab,
            ref location,
            shows,
            ..
        } = viewed.stamp;
        let Some(job) = self.view_jobs.finish(&viewed.stamp) else {
            return false;
        };
        let is_shown = self
            .state
            .tab(tab)
            .is_some_and(|t| t.location() == location && t.listing().is_some());
        if !is_shown {
            return false;
        }
        let is_same_rows = self
            .full
            .get(&tab)
            .is_some_and(|full| Arc::ptr_eq(&full.details, &job.rows));
        if !is_same_rows || shows != self.visibility() {
            let fallback = job.carry.fallback.clone();
            let again = self.submit_view(tab, ViewChange::Again, Some(job));
            if !again {
                self.store_filter(tab, location, fallback);
            }
            return again;
        }
        self.apply_viewed(viewed, job)
    }

    fn apply_viewed(&mut self, viewed: Viewed, job: ViewJob) -> bool {
        let Viewed {
            stamp,
            text,
            full,
            shown,
        } = viewed;
        let tab = stamp.tab;
        if let Some(full) = full {
            self.full.insert(tab, full);
        }
        self.store_filter(tab, &stamp.location, text);
        self.show_filtered(tab, stamp.location, shown);
        self.show_sizes(tab, &job.resized);
        #[cfg(test)]
        counters::bump(&counters::APPLIED);
        if job.carry.lands {
            self.land_cursor(tab);
        }
        true
    }
}

fn next_carry(change: &ViewChange, previous: &str, pending: Option<Carry>) -> Carry {
    let carry = pending.unwrap_or_else(|| Carry {
        sorts: false,
        lands: false,
        fallback: previous.to_owned(),
    });
    Carry {
        sorts: carry.sorts || matches!(change, ViewChange::Sort),
        lands: carry.lands || matches!(change, ViewChange::Text(_)),
        fallback: carry.fallback,
    }
}

#[cfg(test)]
mod counters {
    use std::cell::Cell;
    use std::thread::LocalKey;

    thread_local! {
        pub(super) static SUBMITTED: Cell<usize> = const { Cell::new(0) };
        pub(super) static APPLIED: Cell<usize> = const { Cell::new(0) };
    }

    pub(super) fn bump(counter: &'static LocalKey<Cell<usize>>) {
        counter.with(|count| count.set(count.get() + 1));
    }
}

#[cfg(test)]
fn submitted() -> usize {
    counters::SUBMITTED.with(std::cell::Cell::take)
}

#[cfg(test)]
fn applied() -> usize {
    counters::APPLIED.with(std::cell::Cell::take)
}

#[cfg(test)]
mod tests;
