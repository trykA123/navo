use std::collections::HashMap;
use std::path::PathBuf;

use navo_store::GitSettings;
use navo_ui::git::{BadgeReason, RepoBadge};

use super::ports::{Outcome, Ports};
use crate::git::scan::{Cancel, ScanRepo};

const MINUTE_MS: i64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    Ran,
    Busy,
    Idle,
}

pub(super) struct Engine {
    ports: Ports,
    interval_ms: i64,
    network: bool,
    attempts: HashMap<PathBuf, i64>,
}

impl Engine {
    pub(super) fn new(ports: Ports, settings: &GitSettings) -> Self {
        Self {
            ports,
            interval_ms: i64::from(settings.fetch_minutes) * MINUTE_MS,
            network: settings.network,
            attempts: HashMap::new(),
        }
    }

    /// Fetches at most one due repository; `Busy` means every process slot is taken.
    pub(super) fn tick(&mut self, cancel: &Cancel) -> Step {
        let now = self.ports.clock.now_ms();
        let interval = self.interval_ms;
        self.attempts
            .retain(|_, at| now.saturating_sub(*at) < interval);
        let Some(repo) = self.next_due(now) else {
            return Step::Idle;
        };
        let Some(slot) = self.ports.slots.try_acquire() else {
            return Step::Busy;
        };
        let outcome = self.ports.fetcher.fetch(&repo.path, cancel);
        drop(slot);
        if outcome == Outcome::Cancelled {
            return Step::Idle;
        }
        self.attempts.insert(repo.path, now);
        if outcome == Outcome::Fetched {
            self.ports.watch.rescan(&repo.listing);
        }
        Step::Ran
    }

    fn next_due(&self, now: i64) -> Option<ScanRepo> {
        let mut due: Vec<ScanRepo> = self
            .ports
            .watch
            .repos()
            .into_iter()
            .filter(|repo| fetchable(&repo.badge) && self.is_due(repo, now))
            .collect();
        due.sort_by(|a, b| a.path.cmp(&b.path));
        due.into_iter().find(|repo| {
            self.network
                || self.ports.watch.drive_class(&repo.listing) != navo_fs::DriveClass::Network
        })
    }

    fn is_due(&self, repo: &ScanRepo, now: i64) -> bool {
        let last = repo
            .badge
            .fetched_ms
            .into_iter()
            .chain(self.attempts.get(&repo.path).copied())
            .max();
        last.is_none_or(|last| now.saturating_sub(last) >= self.interval_ms)
    }
}

fn fetchable(badge: &RepoBadge) -> bool {
    badge.upstream.is_some()
        && !matches!(
            badge.reason,
            Some(
                BadgeReason::NoGit
                    | BadgeReason::Ownership
                    | BadgeReason::Filter
                    | BadgeReason::Program
                    | BadgeReason::Guard
            )
        )
}
