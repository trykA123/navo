use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use rusty_core::Location;

use super::look::Looker;
use super::patch::{Patch, Verdict};
use super::recount::Recounts;
use super::{LiveSink, LiveSnapshot};
use crate::measure::profile::{FolderProfile, ProfileSink, ProfileState};
use crate::measure::runner::{ProfileHandle, spawn_profile};
use crate::{CancelToken, IoQueue, Provider, Watch, WatchChange, WatchEvent, WatchSink};

const PENDING_LIMIT: usize = 4_096;
const APPLY_CHUNK: usize = 256;
const LIVE_POLL: Duration = Duration::from_millis(50);

pub(super) enum Note {
    Watched(WatchEvent),
    Counted { run: u64, profile: FolderProfile },
    Recount,
    Stop,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Trigger {
    Asked,
    Automatic,
}

struct Count {
    run: u64,
    started: Instant,
    _handle: ProfileHandle,
}

struct CountSink {
    notes: Sender<Note>,
    run: u64,
    every: Duration,
}

impl ProfileSink for CountSink {
    fn snapshot(&mut self, profile: &FolderProfile) {
        let _ = self.notes.send(Note::Counted {
            run: self.run,
            profile: profile.clone(),
        });
    }

    fn every(&self) -> Duration {
        self.every
    }
}

pub(super) struct Parts<S> {
    pub provider: Arc<dyn Provider>,
    pub folder: Location,
    pub base: Option<FolderProfile>,
    pub queue: Arc<IoQueue>,
    pub sink: S,
    pub notes: Sender<Note>,
    pub cancel: CancelToken,
}

pub(super) struct Session<S> {
    parts: Parts<S>,
    separator: char,
    patch: Option<Patch>,
    progress: Option<FolderProfile>,
    count: Option<Count>,
    recounts: Recounts,
    rest_until: Option<Instant>,
    is_deferred: bool,
    watch: Option<Watch>,
    was_live: bool,
    pending: Vec<WatchChange>,
    counts: u64,
    changes: u64,
    is_dirty: bool,
    published: Option<Instant>,
}

impl<S: LiveSink> Session<S> {
    pub(super) fn new(mut parts: Parts<S>) -> Self {
        let separator = if parts.folder.as_str().contains('\\') {
            '\\'
        } else {
            '/'
        };
        let patch = parts.base.take().map(|base| Patch::new(base, separator));
        Self {
            parts,
            separator,
            patch,
            progress: None,
            count: None,
            recounts: Recounts::default(),
            rest_until: None,
            is_deferred: false,
            watch: None,
            was_live: false,
            pending: Vec::new(),
            counts: 0,
            changes: 0,
            is_dirty: true,
            published: None,
        }
    }

    pub(super) fn run(mut self, notes: &Receiver<Note>) {
        self.start_watch();
        if self.patch.is_none() {
            self.trigger(Trigger::Asked);
        }
        self.publish();
        while !self.parts.cancel.is_cancelled() {
            let next = match self.deadline() {
                Some(at) => notes.recv_timeout(at.saturating_duration_since(Instant::now())),
                None => notes.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            match next {
                Ok(Note::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Ok(note) => self.take(note),
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.apply_pending();
            self.start_when_rested();
            self.publish_when_due();
        }
    }

    fn start_watch(&mut self) {
        let notes = self.parts.notes.clone();
        let sink: WatchSink = Box::new(move |event| {
            let _ = notes.send(Note::Watched(event));
        });
        self.watch = self
            .parts
            .provider
            .watch_tree(&self.parts.folder, sink)
            .ok();
    }

    fn is_live(&self) -> bool {
        self.watch.as_ref().is_some_and(Watch::is_live)
    }

    fn deadline(&self) -> Option<Instant> {
        let refresh = self.parts.sink.refresh_every();
        let publish = self
            .is_dirty
            .then(|| self.published.map_or_else(Instant::now, |at| at + refresh));
        let rested = self.rest_until.filter(|_| self.is_deferred);
        let poll = (self.watch.is_some() && !self.was_live).then(|| Instant::now() + LIVE_POLL);
        let apply = (self.count.is_none() && !self.pending.is_empty()).then(Instant::now);
        [publish, rested, poll, apply].into_iter().flatten().min()
    }

    fn take(&mut self, note: Note) {
        match note {
            Note::Watched(WatchEvent::Changed(changes)) => self.changed(changes),
            Note::Watched(WatchEvent::Overflow) => self.trigger(Trigger::Automatic),
            Note::Watched(WatchEvent::Failed(_)) => {
                self.watch = None;
                self.is_dirty = true;
            }
            Note::Counted { run, profile } => self.counted(run, profile),
            Note::Recount => self.trigger(Trigger::Asked),
            Note::Stop => {}
        }
    }

    fn changed(&mut self, changes: Vec<WatchChange>) {
        self.changes += changes.len() as u64;
        self.is_dirty = true;
        if self.count.is_some() {
            self.recounts.note_change();
            return;
        }
        self.pending.extend(changes);
        if self.pending.len() > PENDING_LIMIT {
            self.trigger(Trigger::Automatic);
        }
    }

    fn trigger(&mut self, trigger: Trigger) {
        self.pending.clear();
        if self.recounts.is_running() {
            self.recounts.request();
            return;
        }
        let is_resting = self.rest_until.is_some_and(|at| Instant::now() < at);
        if trigger == Trigger::Automatic && is_resting {
            self.is_deferred = true;
            return;
        }
        if self.recounts.request() {
            self.start_count();
        }
    }

    fn start_when_rested(&mut self) {
        let is_rested = self.rest_until.is_none_or(|at| Instant::now() >= at);
        if self.is_deferred && is_rested && self.count.is_none() {
            self.is_deferred = false;
            if self.recounts.request() {
                self.start_count();
            }
        }
    }

    fn start_count(&mut self) {
        self.counts += 1;
        self.is_dirty = true;
        let every = if self.patch.is_some() {
            Duration::MAX
        } else {
            self.parts.sink.progress_every()
        };
        let sink = CountSink {
            notes: self.parts.notes.clone(),
            run: self.counts,
            every,
        };
        let handle = spawn_profile(
            &self.parts.queue,
            Arc::clone(&self.parts.provider),
            self.parts.folder.clone(),
            sink,
        );
        self.count = Some(Count {
            run: self.counts,
            started: Instant::now(),
            _handle: handle,
        });
    }

    fn counted(&mut self, run: u64, profile: FolderProfile) {
        let Some(started) = self
            .count
            .as_ref()
            .filter(|count| count.run == run)
            .map(|count| count.started)
        else {
            return;
        };
        if profile.state == ProfileState::Running {
            if self.patch.is_none() {
                self.progress = Some(profile);
                self.is_dirty = true;
                self.publish();
            }
            return;
        }
        self.count = None;
        self.rest_until = Some(Instant::now() + started.elapsed());
        match profile.state {
            ProfileState::Done => {
                self.patch = Some(Patch::new(profile, self.separator));
                self.progress = None;
            }
            _ if self.patch.is_none() => self.progress = Some(profile),
            _ => {}
        }
        self.pending.clear();
        if self.recounts.finish() {
            self.start_count();
        }
        self.is_dirty = true;
        self.publish();
    }

    fn apply_pending(&mut self) {
        if self.count.is_some() || self.pending.is_empty() {
            return;
        }
        let Some(patch) = self.patch.as_mut() else {
            self.pending.clear();
            return;
        };
        let chunk = APPLY_CHUNK.min(self.pending.len());
        let changes: Vec<WatchChange> = self.pending.drain(..chunk).collect();
        let looker = Looker {
            provider: self.parts.provider.as_ref(),
            root: &self.parts.folder,
            separator: self.separator,
            cancel: &self.parts.cancel,
        };
        let needs_recount = changes.iter().any(|change| {
            let look = looker.look(change, patch.room());
            patch.apply(change, look) == Verdict::Recount
        });
        self.is_dirty = true;
        if needs_recount {
            self.trigger(Trigger::Automatic);
        }
    }

    fn publish_when_due(&mut self) {
        let is_live = self.is_live();
        if is_live != self.was_live {
            self.was_live = is_live;
            self.is_dirty = true;
        }
        let refresh = self.parts.sink.refresh_every();
        let is_due = self.published.is_none_or(|at| at.elapsed() >= refresh);
        if self.is_dirty && is_due {
            self.publish();
        }
    }

    fn publish(&mut self) {
        let profile = match (&self.patch, &self.progress) {
            (Some(patch), _) => patch.view(),
            (None, Some(progress)) => progress.clone(),
            (None, None) => return,
        };
        self.was_live = self.is_live();
        let snapshot = LiveSnapshot {
            profile,
            is_live: self.was_live,
            is_recounting: self.count.is_some() && self.patch.is_some(),
            counts: self.counts,
            changes: self.changes,
        };
        self.parts.sink.snapshot(&snapshot);
        self.published = Some(Instant::now());
        self.is_dirty = false;
    }
}
