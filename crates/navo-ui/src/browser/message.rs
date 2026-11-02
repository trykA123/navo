use navo_core::{Location, TabId};
use navo_fs::WatchEvent;

use super::operate::JobUpdate;
use super::{Browser, Loaded, cached, find, measure, naming, view_jobs};

pub(super) enum Message {
    Loaded(Loaded),
    CachedSorted(cached::CachedSorted),
    Viewed(view_jobs::Viewed),
    Measured(navo_fs::OwnerId, measure::Measured),
    ShownKind(navo_fs::OwnerId, TabId, navo_fs::VolumeKind),
    Found(TabId, navo_fs::OwnerId, find::Update),
    Job(JobUpdate),
    Named(naming::Named),
    Watched(Location, WatchEvent),
    Recheck(Location),
    Prefetched(u64),
    Left(TabId, Vec<Location>),
}

impl Message {
    pub(super) fn is_queued(&self) -> bool {
        !matches!(
            self,
            Self::Job(
                JobUpdate::Progress { .. } | JobUpdate::Waiting { .. } | JobUpdate::Done { .. }
            ) | Self::Measured(..)
                | Self::ShownKind(..)
                | Self::Found(..)
                | Self::Watched(..)
                | Self::Recheck(_)
                | Self::Prefetched(_)
                | Self::CachedSorted(_)
                | Self::Viewed(_)
        )
    }
}

impl Browser {
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        let mut sizes = Vec::new();
        while let Ok(message) = self.receiver.try_recv() {
            if message.is_queued() {
                self.in_flight = self.in_flight.saturating_sub(1);
            }
            if let Message::Measured(owner, measured) = message {
                sizes.push((owner, measured));
                continue;
            }
            changed |= self.accept_sizes(std::mem::take(&mut sizes));
            changed |= self.accept_message(message);
        }
        changed |= self.accept_sizes(sizes);
        if changed && crate::perf::is_recording() {
            self.note_folder_shown();
        }
        changed
    }

    fn accept_message(&mut self, message: Message) -> bool {
        match message {
            Message::Loaded(loaded) => self.accept(loaded),
            Message::CachedSorted(early) => self.accept_cached_sorted(early),
            Message::Viewed(viewed) => self.accept_viewed(viewed),
            Message::Measured(owner, measured) => self.accept_sizes(vec![(owner, measured)]),
            Message::ShownKind(owner, tab, kind) => {
                self.accept_shown_kind(owner, tab, kind);
                false
            }
            Message::Found(tab, owner, update) => self.accept_find(tab, owner, update),
            Message::Job(update) => self.accept_job(update),
            Message::Named(named) => self.accept_named(named),
            Message::Watched(location, event) => self.accept_watched(&location, event),
            Message::Recheck(location) => self.accept_recheck(&location),
            Message::Prefetched(id) => self.prefetched(id),
            Message::Left(tab, gone) => self.accept_left(tab, &gone),
        }
    }
}
