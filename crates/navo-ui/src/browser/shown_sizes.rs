use std::sync::Arc;

use navo_core::TabId;
use navo_fs::{
    Classifier, JobKind, JobSlot, OwnerId, Priority, VolumeId, VolumeKind, drive_class, volume_of,
};

use super::measure::Asker;
use super::{Browser, Message};

const MOST_PER_LISTING: usize = 500;

/// D7: with the setting on, the folders shown in a listing on a local drive measure themselves.
pub(super) struct ShownSizes {
    is_on: bool,
    kind_of: Classifier,
}

impl Default for ShownSizes {
    fn default() -> Self {
        Self {
            is_on: false,
            kind_of: Arc::new(volume_kind),
        }
    }
}

fn volume_kind(volume: &VolumeId) -> VolumeKind {
    volume
        .root()
        .map_or(VolumeKind::Unknown, |root| drive_class(&root).into())
}

fn is_local(kind: VolumeKind) -> bool {
    matches!(kind, VolumeKind::Ssd | VolumeKind::Hdd)
}

impl Browser {
    pub fn set_measures_shown_folders(&mut self, is_on: bool) {
        if self.sizes.shown.is_on == is_on {
            return;
        }
        self.sizes.shown.is_on = is_on;
        let tabs: Vec<TabId> = self.sizes.work.keys().copied().collect();
        for tab in tabs {
            if is_on {
                self.plan_shown_sizes(tab);
            } else {
                self.stop_shown_sizes(tab);
            }
        }
    }

    /// Cancels the queued shown-folder sizes; folders a Space asked for are measured again on their own.
    fn stop_shown_sizes(&mut self, tab: TabId) {
        let Some(work) = self.sizes.work.get_mut(&tab) else {
            return;
        };
        work.shown_owner = None;
        work.shown_pending = 0;
        work.shown_planned = false;
        let spaced: Vec<String> = work
            .asked
            .iter()
            .filter(|(_, asked)| asked.asker == Asker::ShownThenSpace)
            .map(|(name, _)| name.clone())
            .collect();
        work.asked.retain(|_, asked| asked.asker == Asker::Space);
        let Some(tab) = self.state.tab(tab).cloned() else {
            return;
        };
        for name in spaced {
            self.measure_entry(&tab, name, Asker::Space);
        }
    }

    #[cfg(test)]
    pub(super) fn set_volume_kinds(&mut self, kind_of: Classifier) {
        self.sizes.shown.kind_of = kind_of;
    }

    /// Classifies the volume off the UI thread; the answer comes back as `Message::ShownKind`.
    pub(super) fn plan_shown_sizes(&mut self, tab: TabId) {
        if !self.sizes.shown.is_on || !self.shows_folders(tab) {
            return;
        }
        let queue = self.pools.queue(JobKind::Size);
        let Some(work) = self.sizes.work.get_mut(&tab) else {
            return;
        };
        if work.shown_planned {
            return;
        }
        work.shown_planned = true;
        work.shown_pending += 1;
        let owner = work.shown_owner.get_or_insert_with(|| queue.owner()).id();
        let volume = volume_of(&work.location);
        let slot = JobSlot {
            volume: volume.clone(),
            priority: Priority::Idle,
            owner,
        };
        let kind_of = Arc::clone(&self.sizes.shown.kind_of);
        let (sender, wake) = (self.sender.clone(), Arc::clone(&self.wake));
        queue.submit_then(
            slot,
            move |_| kind_of(&volume),
            move |kind| {
                let _ = sender.send(Message::ShownKind(owner, tab, kind));
                wake();
            },
        );
    }

    pub(super) fn accept_shown_kind(&mut self, owner: OwnerId, tab: TabId, kind: VolumeKind) {
        let Some(work) = self
            .sizes
            .work
            .get_mut(&tab)
            .filter(|work| work.owns(owner))
        else {
            return;
        };
        work.shown_pending = work.shown_pending.saturating_sub(1);
        if !self.sizes.shown.is_on || !is_local(kind) {
            return;
        }
        let Some(tab) = self.state.tab(tab).cloned() else {
            return;
        };
        let names: Vec<String> = tab
            .listing()
            .map(|listing| {
                listing
                    .entries()
                    .iter()
                    .filter(|entry| entry.is_folder)
                    .take(MOST_PER_LISTING)
                    .map(|entry| entry.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        for name in names {
            self.measure_entry(&tab, name, Asker::Shown);
        }
    }

    fn shows_folders(&self, tab: TabId) -> bool {
        self.state
            .tab(tab)
            .and_then(|tab| tab.listing())
            .is_some_and(|listing| listing.entries().iter().any(|entry| entry.is_folder))
    }
}
