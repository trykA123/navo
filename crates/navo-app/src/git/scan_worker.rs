use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use navo_core::Location;
use navo_fs::DriveClass;
use navo_git::status::RepoStatus;

use super::cache::Cache;
use super::listing::{self, Discovered};
use super::plan::{Check, Input, Plan, Timer, fill};
use super::queue::{Listing, Request, Shared};
use super::skip::MAX_PROOF_AGE;
use super::slots::Slot;
use super::{Cancel, Limits, Notify, Ports, STOP_WAIT, SavedBadges, ScanError, Slots, TICK};

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
    check: Check,
    deadline: Instant,
    timed_out: bool,
    _slot: Slot,
}

struct Done {
    listing: Arc<Listing>,
    root: PathBuf,
    check: Check,
    result: Result<RepoStatus, ScanError>,
}

struct Scanner {
    shared: State,
    ports: Ports,
    config: Config,
    cache: Cache,
    plan: Plan,
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
            let cache = primed(ports.saved.as_deref());
            let plan = Plan::new(ports.probe.clone());
            let mut scanner = Scanner {
                shared,
                ports,
                config,
                cache,
                plan,
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
            self.due();
            self.listings
                .retain(|_, found| !found.listing.cancelled.is_cancelled());
            self.plan.retain(&self.listings);
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

    /// Sleeps until a request, a stop or a deadline; polls only while a call runs or a request
    /// waits for a slot.
    fn wait(&self) {
        let shared = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
        if shared.stopped {
            return;
        }
        if !self.calls.is_empty() || !shared.queue.is_empty() {
            let _wait = self.shared.1.wait_timeout(shared, TICK);
            return;
        }
        if let Some(next) = self.plan.next() {
            let wait = next.saturating_duration_since(self.ports.probe.now());
            #[cfg(test)]
            let shared = mark_parked(shared, true);
            let (shared, _) = self
                .shared
                .1
                .wait_timeout(shared, wait)
                .unwrap_or_else(PoisonError::into_inner);
            #[cfg(test)]
            let shared = mark_parked(shared, false);
            drop(shared);
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
        let (forced, shows) = {
            let mut shared = self.lock();
            (shared.take_forced(&listing), shared.shows_badges(&listing))
        };
        let decision = self
            .plan
            .plan(&mut self.cache, &found, Input { forced, shows });
        let revision = self.plan.publish(&self.shared.0, &found, decision);
        self.listings.insert(listing.folder.clone(), found);
        self.notify(revision);
    }

    /// Runs the deadlines that are due, dropping those whose listing is gone.
    fn due(&mut self) {
        let now = self.ports.probe.now();
        for timer in self.plan.pop_due(now) {
            let listing = timer.listing();
            let Some(found) = self.listings.get(&listing.folder) else {
                continue;
            };
            if !Arc::ptr_eq(&found.listing, &listing) {
                continue;
            }
            let revision = match timer {
                timer @ Timer::Confirm { .. } => self.plan.confirm(&self.shared.0, found, timer),
                timer @ Timer::Expire { .. } => self.plan.expire(&self.shared.0, found, timer),
            };
            self.notify(revision);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.0.lock().unwrap_or_else(PoisonError::into_inner)
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
        let upstream = self
            .cache
            .get(&root)
            .and_then(|cached| cached.root.upstream.clone());
        let check = self.plan.check(&root, upstream);
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
                check,
                deadline: Instant::now() + self.config.limits.status_timeout,
                timed_out: false,
                _slot: slot,
            }),
            Err(error) => {
                tracing::warn!(%error, "cannot start Git status worker");
                self.finish(Done {
                    listing,
                    root,
                    check,
                    result: Err(ScanError::Failed),
                });
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
            self.finish(Done {
                listing: call.listing,
                root: call.root,
                check: call.check,
                result,
            });
        }
    }

    fn timeout(&mut self, index: usize) {
        let call = &mut self.calls[index];
        if call.timed_out || Instant::now() < call.deadline {
            return;
        }
        call.timed_out = true;
        call.cancel.cancel();
        let done = Done {
            listing: call.listing.clone(),
            root: call.root.clone(),
            check: call.check.clone(),
            result: Err(ScanError::TimedOut),
        };
        self.finish(done);
    }

    fn finish(&mut self, done: Done) {
        let Done {
            listing,
            root,
            check,
            result,
        } = done;
        let Some(found) = self.listings.get(&listing.folder) else {
            return;
        };
        if !Arc::ptr_eq(&found.listing, &listing) {
            return;
        }
        let cached = listing::project(
            result.as_ref().map_err(|error| *error),
            &found.bindings,
            &root,
        );
        let at = self.ports.probe.now();
        self.plan.expire_at(&listing, &root, at + MAX_PROOF_AGE);
        let revision = {
            let mut shared = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
            if !shared.is_current(&listing) {
                return;
            }
            let mut badges = shared
                .active
                .get(&listing.folder)
                .and_then(|e| e.badges.as_deref())
                .cloned()
                .unwrap_or_default();
            fill(&mut badges, found, &root, &cached);
            shared.publish(&listing, badges)
        };
        self.plan.finish(&root, check, &result);
        if result.is_ok() {
            if let Some(saved) = &self.ports.saved {
                saved.save(&root, &cached);
            }
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

fn primed(saved: Option<&dyn SavedBadges>) -> Cache {
    let mut cache = Cache::default();
    for (root, cached) in saved.map(SavedBadges::load).unwrap_or_default() {
        cache.insert(root, cached);
    }
    cache
}
