//! The scan plan: what to publish at once, which roots to confirm, and what to scan.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use navo_git::status::{Fingerprint, RepoStatus};
use navo_ui::git::FolderBadges;

use super::cache::{Cache, Cached};
use super::listing::{self, Discovered};
use super::queue::{Forced, Listing, Request, Shared};
use super::skip::{self, CONFIRM_WAIT, Deadlines, Facts, Kept, Proofs, Verdict};
use super::watches::{Watches, root_key};
use super::{Probe, ScanError};

/// What the scanner takes from the shared queue before a plan.
pub(super) struct Input {
    pub forced: Forced,
    pub shows: bool,
}

/// What discovery decided: reuse these roots after a confirm, scan the rest.
pub(super) struct Decision {
    shows: bool,
    keep: bool,
    updates: Vec<(PathBuf, Cached)>,
    confirm: Vec<PathBuf>,
    scan: Vec<PathBuf>,
    at: Instant,
}

/// A deadline the scanner owns.
pub(super) enum Timer {
    Confirm {
        listing: Arc<Listing>,
        root: PathBuf,
        /// The plan time, so the one-second wait does not age the proof on its own.
        at: Instant,
    },
}

impl Timer {
    pub(super) fn listing(&self) -> Arc<Listing> {
        match self {
            Self::Confirm { listing, .. } => listing.clone(),
        }
    }
}

/// The marks and proofs a scanner keeps between its runs.
pub(super) struct Plan {
    probe: Arc<dyn Probe>,
    proofs: Proofs,
    watches: Watches,
    deadlines: Deadlines<Timer>,
}

/// The watch mark, fingerprint and time a status call started from.
#[derive(Clone)]
pub(super) struct Check {
    mark: Option<skip::Mark>,
    print: Option<Fingerprint>,
    upstream: Option<String>,
    checked_at: Instant,
}

impl Plan {
    pub(super) fn new(probe: Arc<dyn Probe>) -> Self {
        Self {
            watches: Watches::new(probe.clone()),
            probe,
            proofs: Proofs::default(),
            deadlines: Deadlines::default(),
        }
    }

    /// The next deadline, if any.
    pub(super) fn next(&self) -> Option<Instant> {
        self.deadlines.next()
    }

    pub(super) fn pop_due(&mut self, now: Instant) -> Vec<Timer> {
        self.deadlines.pop_due(now)
    }

    /// Phase one: picks the roots to publish stale now and confirm one second later.
    pub(super) fn plan(&mut self, cache: &mut Cache, found: &Discovered, input: Input) -> Decision {
        let now = self.probe.now();
        let matches = found
            .roots
            .iter()
            .any(|root| input.forced.roots.contains(&root_key(root)));
        let force_all = input.forced.all || (!input.forced.roots.is_empty() && !matches);
        let mut updates = Vec::new();
        let mut confirm = Vec::new();
        let mut scan = Vec::new();
        for root in &found.roots {
            let class = self.probe.root_class(root, &found.listing.folder);
            let key = root_key(root);
            let mark = self.watches.mark(root);
            if mark.is_none() {
                self.watches.arm(root, class);
            }
            let facts = Facts {
                forced: force_all || input.forced.roots.contains(&key),
                truncated: found.truncated,
                class,
                kept: self.proofs.get(&key),
                mark,
                now,
            };
            if let (Ok(()), Some(kept)) = (skip::precheck(&facts), facts.kept.as_ref()) {
                let cached = listing::project(Ok(&kept.status), &found.bindings, root);
                updates.push((root.clone(), cached));
                confirm.push(root.clone());
            } else {
                scan.push(root.clone());
                updates.extend(hidden(cache, input.shows, root));
            }
        }
        let keep = !found.slow && !found.roots.is_empty() && input.shows && updates.is_empty();
        Decision {
            shows: input.shows,
            keep,
            updates,
            confirm,
            scan,
            at: now,
        }
    }

    /// Publishes the decision, queues the scans, and schedules the confirms.
    pub(super) fn publish(
        &mut self,
        shared: &Mutex<Shared>,
        found: &Discovered,
        mut decision: Decision,
    ) -> Option<u64> {
        let listing = &found.listing;
        let revision = {
            let mut shared = shared.lock().unwrap_or_else(PoisonError::into_inner);
            if decision.keep {
                shared.push_all(listing, &decision.scan);
                return None;
            }
            let mut badges = if decision.shows && !found.roots.is_empty() {
                shared
                    .active
                    .get(&listing.folder)
                    .and_then(|entry| entry.badges.as_deref())
                    .cloned()
                    .unwrap_or_default()
            } else {
                FolderBadges::default()
            };
            badges.slow_drive = found.slow;
            for (root, cached) in &mut decision.updates {
                cached.stale = true;
                fill(&mut badges, found, root, cached);
            }
            let revision = shared.publish(listing, badges);
            if revision.is_some() {
                shared.push_all(listing, &decision.scan);
            }
            revision
        };
        if revision.is_some() {
            for root in decision.confirm {
                self.deadlines.insert(
                    decision.at + CONFIRM_WAIT,
                    Timer::Confirm {
                        listing: listing.clone(),
                        root,
                        at: decision.at,
                    },
                );
            }
        }
        revision
    }

    /// Reads what a status call must be checked against before it starts.
    pub(super) fn check(&mut self, root: &Path, upstream: Option<String>) -> Check {
        let mark = self.watches.mark(root);
        let print = mark.and_then(|_| self.probe.fingerprint(root, upstream.as_deref()));
        Check {
            mark,
            print,
            upstream,
            checked_at: self.probe.now(),
        }
    }

    /// Keeps the status only when the fingerprint, mark and upstream still agree.
    pub(super) fn finish(
        &mut self,
        root: &Path,
        check: Check,
        result: &Result<RepoStatus, ScanError>,
    ) {
        let key = root_key(root);
        let Ok(status) = result else {
            self.proofs.remove(&key);
            return;
        };
        let status_upstream = status
            .upstream
            .as_ref()
            .map(|upstream| upstream.name.as_str());
        let post = if check.mark.is_some() && check.print.is_some() {
            self.probe.fingerprint(root, check.upstream.as_deref())
        } else {
            None
        };
        let proof = skip::proof(
            check.mark,
            check.print,
            post,
            check.upstream.as_deref(),
            status_upstream,
            check.checked_at,
        );
        match proof {
            Some(proof) => {
                self.proofs.insert(
                    key,
                    Kept {
                        proof,
                        status: Arc::new(status.clone()),
                    },
                );
            }
            None => {
                self.proofs.remove(&key);
            }
        }
    }

    /// Phase two: confirms one root, or queues its status when the proof no longer holds.
    pub(super) fn confirm(
        &mut self,
        shared: &Mutex<Shared>,
        found: &Discovered,
        timer: Timer,
    ) -> Option<u64> {
        let Timer::Confirm { listing, root, at } = timer;
        {
            let shared = shared.lock().unwrap_or_else(PoisonError::into_inner);
            if !shared.is_current(&listing) {
                return None;
            }
        }
        let key = root_key(&root);
        let class = self.probe.root_class(&root, &listing.folder);
        let facts = Facts {
            forced: false,
            truncated: found.truncated,
            class,
            kept: self.proofs.get(&key),
            mark: self.watches.mark(&root),
            now: at,
        };
        let upstream = facts
            .kept
            .as_ref()
            .and_then(|kept| kept.proof.upstream.clone());
        let verdict = skip::confirm(&facts, || {
            self.probe.fingerprint(&root, upstream.as_deref())
        });
        if verdict != Verdict::Reuse {
            let mut shared = shared.lock().unwrap_or_else(PoisonError::into_inner);
            if shared.is_current(&listing) {
                shared.push(Request::Status(listing, root));
            }
            return None;
        }
        let kept = facts.kept.as_ref()?;
        let cached = listing::project(Ok(&kept.status), &found.bindings, &root);
        publish_fresh(shared, found, &root, &cached)
    }
}

fn publish_fresh(
    shared: &Mutex<Shared>,
    found: &Discovered,
    root: &Path,
    cached: &Cached,
) -> Option<u64> {
    let mut shared = shared.lock().unwrap_or_else(PoisonError::into_inner);
    if !shared.is_current(&found.listing) {
        return None;
    }
    let mut badges = shared
        .active
        .get(&found.listing.folder)
        .and_then(|entry| entry.badges.as_deref())
        .cloned()
        .unwrap_or_default();
    fill(&mut badges, found, root, cached);
    shared.publish(&found.listing, badges)
}

/// A cached projection of a scanned root, hidden while it is scanned.
fn hidden(cache: &mut Cache, shows: bool, root: &Path) -> Option<(PathBuf, Cached)> {
    if shows {
        return None;
    }
    stale(cache, root)
}

/// A cached projection of a root, marked stale.
fn stale(cache: &mut Cache, root: &Path) -> Option<(PathBuf, Cached)> {
    let mut cached = cache.get(root)?;
    cached.stale = true;
    Some((root.to_path_buf(), cached))
}

pub(super) fn fill(badges: &mut FolderBadges, found: &Discovered, root: &Path, cached: &Cached) {
    if found.parent.as_deref() == Some(root) {
        badges.repository = Some(cached.repo());
    }
    for binding in found.bindings.iter().filter(|binding| binding.root == root) {
        if let Some(badge) = cached.row(&binding.relative) {
            if !badge.is_root {
                badges.roots.insert(binding.name.clone(), cached.repo());
            }
            badges.by_name.insert(binding.name.clone(), badge);
        }
    }
}

#[cfg(test)]
#[path = "scan_tests/skip_flow.rs"]
mod tests;
