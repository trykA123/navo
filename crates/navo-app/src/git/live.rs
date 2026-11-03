use std::time::Duration;

use navo_store::{GitEngine, GitSettings};

use super::hooks::{Hooks, Shared, State, lock, rows};
use super::scan::GitScan;

const FOCUS_STALE: Duration = Duration::from_secs(60);

impl Shared {
    fn age(&self) -> Duration {
        (self.clock)().saturating_duration_since(*lock(&self.stamp))
    }
}

impl Hooks {
    /// Starts or stops the scanner when `badges`, `engine`, `network` or `fetch` changed, and
    /// tells the subscribers. A new `fetch_minutes` alone restarts only the fetch timer.
    pub(crate) fn apply(&self, settings: &GitSettings) {
        match self.store(settings) {
            Change::Keep => {}
            Change::Retime => self.retime(settings),
            Change::Restart(old) => {
                drop(old);
                self.restart(settings);
            }
        }
    }

    fn store(&self, settings: &GitSettings) -> Change {
        let mut state = lock(&self.state);
        let previous = std::mem::replace(&mut state.settings, settings.clone());
        if scan_key(&previous) != scan_key(settings) {
            Change::Restart(state.scan.take())
        } else if previous.fetch_minutes != settings.fetch_minutes {
            Change::Retime
        } else {
            Change::Keep
        }
    }

    fn restart(&self, settings: &GitSettings) {
        let started = self.start(settings);
        {
            let mut state = lock(&self.state);
            state.scan = started.scan;
            observe_seen(&state);
        }
        drop(started.retired);
        self.shared.notify_all();
    }

    fn retime(&self, settings: &GitSettings) {
        let retired = {
            let state = lock(&self.state);
            self.swap_timer(state.scan.as_ref(), settings)
        };
        drop(retired);
    }

    /// Rescans every observed folder when the last status is over 60 s old.
    pub(crate) fn focus_regained(&self) {
        if self.shared.age() <= FOCUS_STALE {
            return;
        }
        let state = lock(&self.state);
        let Some(scan) = &state.scan else {
            return;
        };
        self.shared.touch();
        for folder in state.seen.keys() {
            scan.rescan(folder);
        }
    }
}

/// What changed in the settings, for the scanner and the fetch timer.
enum Change {
    /// The old scanner, taken out to be stopped.
    Restart(Option<GitScan>),
    Retime,
    Keep,
}

fn observe_seen(state: &State) {
    let Some(scan) = &state.scan else {
        return;
    };
    for (folder, (revision, names)) in &state.seen {
        scan.observe(folder, *revision, rows(names));
    }
}

fn scan_key(settings: &GitSettings) -> (bool, GitEngine, bool, bool) {
    (
        settings.badges,
        settings.engine,
        settings.network,
        settings.fetch,
    )
}
