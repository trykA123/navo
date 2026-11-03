use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use navo_core::Location;
use navo_fs::DriveClass;
use navo_git::status::RepoStatus;
use navo_ui::git::FolderBadges;

use super::cache::{Cache, Cached};
use super::listing::{self, Discovered};
use super::queue::{Listing, Request, Shared};
use super::slots::Slot;
use super::{Cancel, Limits, Notify, Ports, STOP_WAIT, ScanError, Slots, TICK};

type State = Arc<(Mutex<Shared>, Condvar)>;

pub(super) struct Config {
    pub network: bool,
    pub limits: Limits,
    pub notify: Notify,
    pub slots: Slots,
}

pub(super) struct Running {
    thread: JoinHandle<()>,
    done: Receiver<bool>,
}

impl Running {
    pub fn stop(self) -> bool {
        match self.done.recv_timeout(STOP_WAIT) {
            Ok(stopped) => self.thread.join().is_ok() && stopped,
            Err(error) => {
                tracing::warn!(%error, "git scanner did not stop cleanly");
                false
            }
        }
    }
}

struct Call {
    listing: Arc<Listing>,
    root: PathBuf,
    cancel: Cancel,
    thread: JoinHandle<()>,
    result: Receiver<Result<RepoStatus, ScanError>>,
    deadline: Instant,
    timed_out: bool,
    _slot: Slot,
}

struct Scanner {
    shared: State,
    ports: Ports,
    config: Config,
    cache: Cache,
    listings: HashMap<Location, Discovered>,
    calls: Vec<Call>,
    /// A network probe helper is still running.
    network_probe: Arc<AtomicBool>,
}

pub(super) fn spawn(shared: State, ports: Ports, config: Config) -> io::Result<Running> {
    let (done, receiver) = mpsc::sync_channel(1);
    let thread = thread::Builder::new()
        .name("navo-git-scan".into())
        .spawn(move || {
            let mut scanner = Scanner {
                shared,
                ports,
                config,
                cache: Cache::default(),
                listings: HashMap::new(),
                calls: Vec::new(),
                network_probe: Arc::default(),
            };
            scanner.run();
            let _ = done.send(scanner.stop_calls());
        })?;
    Ok(Running {
        thread,
        done: receiver,
    })
}

impl Scanner {
    fn run(&mut self) {
        loop {
            self.collect();
            self.listings
                .retain(|_, found| !found.listing.cancelled.is_cancelled());
            let slot = self.config.slots.try_acquire();
            let Some(request) = self.next_request(slot.is_some()) else {
                return;
            };
            match (request, slot) {
                (Some(Request::Probe(listing)), _) => self.probe(listing),
                (Some(Request::Status(listing, root)), Some(slot)) => {
                    self.launch(listing, root, slot);
                }
                _ => self.wait(),
            }
        }
    }

    fn next_request(&self, has_slot: bool) -> Option<Option<Request>> {
        let mut shared = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
        (!shared.stopped).then(|| shared.pop(has_slot))
    }

    /// Sleeps until a request or a stop; polls only while a call runs or a request waits for a slot.
    fn wait(&self) {
        let shared = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
        if shared.stopped {
            return;
        }
        if !self.calls.is_empty() || !shared.queue.is_empty() {
            let _wait = self.shared.1.wait_timeout(shared, TICK);
            return;
        }
        #[cfg(test)]
        let shared = mark_parked(shared, true);
        let shared = self
            .shared
            .1
            .wait(shared)
            .unwrap_or_else(PoisonError::into_inner);
        #[cfg(test)]
        let shared = mark_parked(shared, false);
        drop(shared);
    }

    fn probe(&mut self, listing: Arc<Listing>) {
        if listing.cancelled.is_cancelled() {
            return;
        }
        let found = self.discover(&listing);
        let mut badges = FolderBadges::default();
        for root in &found.roots {
            if let Some(mut cached) = self.cache.get(root) {
                cached.stale = true;
                fill(&mut badges, &found, root, &cached);
            }
        }
        let revision = {
            let mut shared = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
            let revision = shared.publish(&listing, badges);
            if revision.is_some() {
                shared.push_all(&listing, &found.roots);
            }
            revision
        };
        self.listings.insert(listing.folder.clone(), found);
        self.notify(revision);
    }

    fn discover(&self, listing: &Arc<Listing>) -> Discovered {
        let (probe, limits) = (&self.ports.probe, self.config.limits);
        if self.config.network && probe.drive_class(&listing.folder) == DriveClass::Network {
            let running = self.network_probe.clone();
            return listing::discover_on_helper(listing.clone(), probe.clone(), limits, running);
        }
        listing::discover(listing.clone(), probe.as_ref(), self.config.network, limits)
    }

    fn launch(&mut self, listing: Arc<Listing>, root: PathBuf, slot: Slot) {
        if listing.cancelled.is_cancelled() {
            return;
        }
        let cancel = listing.start_call();
        if cancel.is_cancelled() {
            return;
        }
        let source = self.ports.source.clone();
        let token = cancel.clone();
        let path = root.clone();
        let (sender, result) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("navo-git-status".into())
            .spawn(move || {
                let outcome = if token.is_cancelled() {
                    Err(ScanError::Cancelled)
                } else {
                    source.status(&path, &token)
                };
                let _ = sender.send(outcome);
            });
        match thread {
            Ok(thread) => self.calls.push(Call {
                listing,
                root,
                cancel,
                thread,
                result,
                deadline: Instant::now() + self.config.limits.status_timeout,
                timed_out: false,
                _slot: slot,
            }),
            Err(error) => {
                tracing::warn!(%error, "cannot start Git status worker");
                self.finish(&listing, root, Err(ScanError::Failed));
            }
        }
    }

    fn collect(&mut self) {
        let mut index = 0;
        while index < self.calls.len() {
            let outcome = self.calls[index].result.try_recv();
            match outcome {
                Ok(result) => self.complete(index, result),
                Err(TryRecvError::Disconnected) => self.complete(index, Err(ScanError::Failed)),
                Err(TryRecvError::Empty) => {
                    self.timeout(index);
                    index += 1;
                }
            }
        }
    }

    fn complete(&mut self, index: usize, result: Result<RepoStatus, ScanError>) {
        let call = self.calls.swap_remove(index);
        call.cancel.cancel();
        if call.thread.join().is_err() {
            tracing::warn!("git status worker panicked");
        }
        if !call.timed_out {
            self.finish(&call.listing, call.root, result.as_ref().map_err(|e| *e));
        }
    }

    fn timeout(&mut self, index: usize) {
        let call = &mut self.calls[index];
        if call.timed_out || Instant::now() < call.deadline {
            return;
        }
        call.timed_out = true;
        call.cancel.cancel();
        let listing = call.listing.clone();
        let root = call.root.clone();
        self.finish(&listing, root, Err(ScanError::TimedOut));
    }

    fn finish(
        &mut self,
        listing: &Arc<Listing>,
        root: PathBuf,
        result: Result<&RepoStatus, ScanError>,
    ) {
        let Some(found) = self.listings.get(&listing.folder) else {
            return;
        };
        if !Arc::ptr_eq(&found.listing, listing) {
            return;
        }
        let cached = listing::project(result, &found.bindings, &root);
        let revision = {
            let mut shared = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
            if !shared.is_current(listing) {
                return;
            }
            let mut badges = shared
                .active
                .get(&listing.folder)
                .and_then(|e| e.badges.as_deref())
                .cloned()
                .unwrap_or_default();
            fill(&mut badges, found, &root, &cached);
            shared.publish(listing, badges)
        };
        if result.is_ok() {
            self.cache.insert(root, cached);
        }
        self.notify(revision);
    }

    fn notify(&self, revision: Option<u64>) {
        if let Some(revision) = revision {
            (self.config.notify)(revision);
        }
    }

    fn stop_calls(&mut self) -> bool {
        for call in &self.calls {
            call.cancel.cancel();
        }
        let deadline = Instant::now() + STOP_WAIT - TICK * 2;
        while !self.calls.is_empty() && Instant::now() < deadline {
            self.collect();
            thread::sleep(TICK.min(deadline.saturating_duration_since(Instant::now())));
        }
        if !self.calls.is_empty() {
            tracing::warn!("git status source ignored cancellation; detaching workers");
        }
        self.calls.is_empty()
    }
}

#[cfg(test)]
fn mark_parked(
    mut shared: std::sync::MutexGuard<'_, Shared>,
    parked: bool,
) -> std::sync::MutexGuard<'_, Shared> {
    shared.parked = parked;
    shared
}

fn fill(badges: &mut FolderBadges, found: &Discovered, root: &Path, cached: &Cached) {
    for binding in found.bindings.iter().filter(|b| b.root == root) {
        if let Some(badge) = cached.row(&binding.relative) {
            badges.by_name.insert(binding.name.clone(), badge);
        }
    }
}
