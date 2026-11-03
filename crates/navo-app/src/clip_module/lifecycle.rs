use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::Duration;

use navo_clipboard::{ClipError, ClipStore, FailKind, Limits, Reply};

use super::running::Running;
use super::{Active, ClipModule, Config, State, lock};
use crate::metrics;

const QUIT_WAIT: Duration = Duration::from_secs(10);

impl State {
    /// Replaces the active part. Revisions reported later stay above every earlier one, and a
    /// start still in flight becomes stale.
    fn retire(&mut self, next: Active) -> Option<Running> {
        self.epoch += match &self.active {
            Active::On(running) => running.service().snapshot().revision + 2,
            Active::Off { .. } | Active::Starting => 1,
        };
        self.generation += 1;
        match std::mem::replace(&mut self.active, next) {
            Active::On(running) => Some(running),
            Active::Off { .. } | Active::Starting => None,
        }
    }

    fn has_notice(&self) -> bool {
        matches!(&self.active, Active::Off { notice: Some(_) })
    }
}

impl ClipModule {
    /// Follows a settings change without blocking: the store opens and closes on a job
    /// thread. Called from one thread only; the newest config wins.
    pub(super) fn apply_config(self: &Arc<Self>, config: Config) {
        let changed = {
            let mut state = lock(&self.state);
            if state.stopped || state.config == Some(config) {
                return;
            }
            state.config = Some(config);
            self.transition(&mut state, config)
        };
        if changed {
            self.listeners.notify();
        }
    }

    /// Returns whether the hooks' answers changed. Jobs are queued under the lock, so they
    /// run in the order of the changes.
    fn transition(self: &Arc<Self>, state: &mut State, config: Config) -> bool {
        match (&state.active, config.enabled) {
            (Active::On(running), true) => {
                running.service().set_recording(config.record);
                false
            }
            (Active::Starting, true) => false,
            (Active::Off { .. }, true) => self.begin_start(state, config),
            (Active::Off { notice: None }, false) => false,
            (_, false) => {
                let changed = !matches!(state.active, Active::Starting);
                if let Some(running) = state.retire(Active::Off { notice: None }) {
                    self.jobs.run("navo-clipboard-stop", move || running.stop());
                }
                changed
            }
        }
    }

    fn begin_start(self: &Arc<Self>, state: &mut State, config: Config) -> bool {
        let had_notice = state.has_notice();
        let Some(db) = self.db.clone() else {
            state.active = Active::Off { notice: None };
            return had_notice;
        };
        state.retire(Active::Starting);
        let generation = state.generation;
        let module = self.clone();
        self.jobs.run("navo-clipboard-start", move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                Running::start(&db, config.record, &module.backends, &module.listeners)
            }))
            .unwrap_or(Err(ClipError::Malformed("the clipboard start panicked")));
            module.finish_start(generation, result);
        });
        had_notice
    }

    /// Installs a started module unless a newer change came first; a stale one is stopped.
    fn finish_start(&self, generation: u64, result: Result<Running, ClipError>) {
        let mut state = lock(&self.state);
        if state.generation != generation {
            drop(state);
            if let Ok(running) = result {
                running.stop();
            }
            return;
        }
        let next = match result {
            Ok(running) => {
                if let Some(config) = state.config {
                    running.service().set_recording(config.record);
                }
                Active::On(running)
            }
            Err(error) => {
                tracing::warn!(%error, "the clipboard history could not be opened");
                Active::Off {
                    notice: Some(navo_l10n::tr("clipboard-open-failed")),
                }
            }
        };
        state.retire(next);
        drop(state);
        metrics::mark("clipboard module");
        self.listeners.notify();
    }

    /// Turns the module off for good, stops it here and waits for its job threads.
    /// A second call does nothing.
    pub(crate) fn stop(&self) {
        let running = {
            let mut state = lock(&self.state);
            state.config = None;
            state.stopped = true;
            match state.active {
                Active::Off { notice: None } => None,
                _ => state.retire(Active::Off { notice: None }),
            }
        };
        if let Some(running) = running {
            running.stop();
        }
        if !self.jobs.wait(QUIT_WAIT) {
            tracing::warn!("the clipboard jobs did not end in time");
        }
    }

    /// "Delete all" while the module is off: wipes the files on a job thread and replies with
    /// the outcome. Replies busy while the module is starting.
    pub(super) fn wipe_while_off(&self) -> Reply<()> {
        let (reply, answer) = channel();
        let state = lock(&self.state);
        if !matches!(state.active, Active::Off { .. }) {
            let _ = reply.send(Err(FailKind::Busy));
            return answer;
        }
        match self.db.clone() {
            Some(db) => self.jobs.run("navo-clipboard-wipe", move || {
                let _ = reply.send(wipe_files(&db));
            }),
            None => {
                let _ = reply.send(Ok(()));
            }
        }
        answer
    }

    #[cfg(test)]
    pub(super) fn settle(&self) {
        assert!(self.jobs.wait(Duration::from_secs(20)), "jobs never ended");
    }
}

/// Nothing to delete without a data folder. A file the store refuses, such as one from a
/// newer version, stays untouched and reads as a failure.
fn wipe_files(db: &Path) -> Result<(), FailKind> {
    if !db.parent().is_some_and(Path::is_dir) {
        return Ok(());
    }
    ClipStore::open(db, Limits::default())
        .and_then(|store| store.wipe_all(db))
        .map(drop)
        .map_err(|error| {
            tracing::warn!(%error, "the clipboard files could not be deleted");
            FailKind::Store
        })
}
