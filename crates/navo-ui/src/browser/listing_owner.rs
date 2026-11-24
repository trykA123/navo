//! Per-tab listing owners: each tab's pending folder listing runs under its own queue owner, so
//! leaving, replacing or closing that request cancels its queued or running job (NV5b0).

use std::collections::HashMap;
use std::sync::mpsc::Sender;

use navo_core::{FsError, FsErrorKind, Location, TabId};
use navo_fs::{JobKind, JobSlot, OwnerGuard, Priority, volume_of};

use super::message::Message;
use super::{Browser, Loaded, Waker};

/// The owner guard of each tab's in-flight listing, with the request id it serves.
#[derive(Default)]
pub(super) struct ListingOwners(HashMap<TabId, (u64, OwnerGuard)>);

/// Sends the listing's `Loaded` once: the real result, or a cancelled one when the job is
/// dropped unrun (cancelled while queued), so the browser's in-flight count still drops.
struct Reply {
    sender: Sender<Message>,
    wake: Waker,
    tab: TabId,
    request: u64,
    location: Option<Location>,
}

impl Reply {
    /// Sends `result` for this request and wakes the UI; a closed channel is ignored.
    fn send(&mut self, result: Result<super::cached::Fresh, FsError>) {
        let Some(location) = self.location.take() else {
            return;
        };
        let _ = self.sender.send(Message::Loaded(Loaded {
            tab: self.tab,
            location,
            request: self.request,
            result,
        }));
        (self.wake)();
    }
}

impl Drop for Reply {
    fn drop(&mut self) {
        self.send(Err(FsError::new(FsErrorKind::Cancelled)));
    }
}

impl ListingOwners {
    /// Holds `owner` as `tab`'s listing owner for `request` (an adopted prefetch's guard).
    pub(super) fn hold(&mut self, tab: TabId, request: u64, owner: OwnerGuard) {
        self.0.insert(tab, (request, owner));
    }
}

impl Browser {
    /// Drops the owner of every listing that is no longer its tab's pending request (left,
    /// replaced or closed); the drop cancels that owner's jobs through the queue.
    pub(super) fn retain_listing_owners(&mut self) {
        let state = &self.state;
        self.listing_owners.0.retain(|tab, (request, _)| {
            state.tab(*tab).and_then(|t| t.pending_request()) == Some(*request)
        });
    }

    /// Queues the listing of `location` for `tab` under a fresh owner held for `request`.
    pub(super) fn submit(&mut self, tab: TabId, location: Location, request: u64) {
        let fetch = self.fetch(tab, &location, request);
        let queue = self.pools.queue(JobKind::Listing);
        let owner = queue.owner();
        let slot = JobSlot {
            volume: volume_of(&location),
            priority: Priority::Visible,
            owner: owner.id(),
        };
        let mut reply = Reply {
            sender: self.sender.clone(),
            wake: std::sync::Arc::clone(&self.wake),
            tab,
            request,
            location: Some(location),
        };
        self.in_flight += 1;
        queue.submit(slot, Box::new(move |cancel| reply.send(fetch.run(cancel))));
        self.listing_owners.0.insert(tab, (request, owner));
    }
}
