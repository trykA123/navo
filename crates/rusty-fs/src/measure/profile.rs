use std::time::{Duration, Instant};

use rusty_core::{FsErrorKind, Location};

use super::FolderSize;
use super::kinds::{Kinds, Tally};
use super::largest::{LargeFile, Largest, Ranking};
use super::now_ms;
use super::walk::{Visit, Walk};
use crate::{CancelToken, FsEntry, Provider};

pub const PROGRESS_EVERY: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileState {
    Running,
    Done,
    Stopped,
    Failed(FsErrorKind),
}

impl ProfileState {
    #[must_use]
    pub fn is_final(self) -> bool {
        self != Self::Running
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderProfile {
    pub totals: FolderSize,
    pub newest_ms: Option<i64>,
    pub kinds: Vec<(String, Tally)>,
    pub other: Tally,
    pub largest: Vec<LargeFile>,
    pub ranking: Ranking,
    pub counted_ms: Option<i64>,
    pub state: ProfileState,
}

pub trait ProfileSink {
    fn snapshot(&mut self, profile: &FolderProfile);

    fn every(&self) -> Duration {
        PROGRESS_EVERY
    }
}

impl<F: FnMut(&FolderProfile)> ProfileSink for F {
    fn snapshot(&mut self, profile: &FolderProfile) {
        self(profile);
    }
}

struct Profiler<'a> {
    root: &'a str,
    newest_ms: Option<i64>,
    kinds: Kinds,
    largest: Largest,
    sink: &'a mut dyn ProfileSink,
    last_sent: Instant,
}

impl Profiler<'_> {
    fn profile(&self, totals: FolderSize, state: ProfileState) -> FolderProfile {
        let (kinds, other) = self.kinds.snapshot();
        let ranking = self.largest.snapshot();
        FolderProfile {
            totals,
            newest_ms: self.newest_ms,
            kinds,
            other,
            largest: ranking.top(),
            ranking,
            counted_ms: state.is_final().then(now_ms),
            state,
        }
    }
}

impl Visit for Profiler<'_> {
    fn file(&mut self, folder: &Location, entry: &FsEntry) {
        self.newest_ms = self.newest_ms.max(entry.modified_ms);
        self.kinds.add(&entry.name.to_text(), entry.size);
        if self.largest.admits(entry.size) {
            let path = relative(self.root, folder, &entry.name.to_text());
            self.largest.offer(LargeFile {
                path,
                bytes: entry.size,
            });
        }
    }

    fn batch_end(&mut self, totals: &FolderSize) {
        if self.last_sent.elapsed() < self.sink.every() {
            return;
        }
        let profile = self.profile(*totals, ProfileState::Running);
        self.sink.snapshot(&profile);
        self.last_sent = Instant::now();
    }
}

pub(super) fn relative(root: &str, folder: &Location, name: &str) -> String {
    let below = folder.as_str().strip_prefix(root).unwrap_or_default();
    let below = below.trim_start_matches(['\\', '/']);
    if below.is_empty() {
        return name.to_owned();
    }
    let separator = if below.contains('\\') || root.contains('\\') {
        '\\'
    } else {
        '/'
    };
    format!("{below}{separator}{name}")
}

pub fn folder_profile(
    provider: &dyn Provider,
    folder: &Location,
    cancel: &CancelToken,
    sink: &mut dyn ProfileSink,
) -> FolderProfile {
    let mut totals = FolderSize::default();
    let mut profiler = Profiler {
        root: folder.as_str(),
        newest_ms: None,
        kinds: Kinds::default(),
        largest: Largest::default(),
        sink,
        last_sent: Instant::now(),
    };
    let walk = Walk {
        provider,
        root: folder,
        cancel,
    };
    let state = match walk.run(&mut totals, &mut profiler) {
        Ok(()) => ProfileState::Done,
        Err(error) if error.kind == FsErrorKind::Cancelled => ProfileState::Stopped,
        Err(error) => ProfileState::Failed(error.kind),
    };
    let last = profiler.profile(totals, state);
    profiler.sink.snapshot(&last);
    last
}
