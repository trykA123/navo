use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use rusty_fs::{FolderProfile, ProfileHandle, ProfileSink, ProfileState};

use super::inbox::Arrival;
use crate::browser::Waker;

pub(in crate::window_view) enum Phase {
    Counting {
        _handle: ProfileHandle,
        started: Instant,
    },
    Done {
        took: Option<Duration>,
    },
    Stopped,
    Failed,
}

pub(in crate::window_view) struct Scan {
    pub profile: Option<FolderProfile>,
    pub phase: Phase,
    run: u64,
}

impl Scan {
    pub(super) fn cached(profile: FolderProfile) -> Self {
        Self {
            profile: Some(profile),
            phase: Phase::Done { took: None },
            run: 0,
        }
    }

    pub(super) fn counting(handle: ProfileHandle, run: u64) -> Self {
        Self {
            profile: None,
            phase: Phase::Counting {
                _handle: handle,
                started: Instant::now(),
            },
            run,
        }
    }

    pub(super) fn resume(&mut self, handle: ProfileHandle, run: u64) {
        self.phase = Phase::Counting {
            _handle: handle,
            started: Instant::now(),
        };
        self.run = run;
    }

    pub(super) fn is_counting(&self) -> bool {
        matches!(self.phase, Phase::Counting { .. })
    }

    pub(super) fn stop(&mut self) -> bool {
        if !self.is_counting() {
            return false;
        }
        self.phase = Phase::Stopped;
        true
    }

    pub(super) fn accept(&mut self, run: u64, profile: FolderProfile) -> bool {
        if run != self.run || !self.is_counting() {
            return false;
        }
        let took = match &self.phase {
            Phase::Counting { started, .. } => started.elapsed(),
            _ => Duration::ZERO,
        };
        let next = match profile.state {
            ProfileState::Running => None,
            ProfileState::Done => Some(Phase::Done { took: Some(took) }),
            ProfileState::Stopped => Some(Phase::Stopped),
            ProfileState::Failed(_) => Some(Phase::Failed),
        };
        if let Some(next) = next {
            self.phase = next;
        }
        self.profile = Some(profile);
        true
    }
}

pub(super) struct Snapshots {
    pub send: Sender<Arrival>,
    pub wake: Waker,
    pub load: u64,
    pub run: u64,
    pub every: Duration,
}

impl ProfileSink for Snapshots {
    fn snapshot(&mut self, profile: &FolderProfile) {
        let arrival = Arrival::Snapshot {
            load: self.load,
            run: self.run,
            profile: profile.clone(),
        };
        if self.send.send(arrival).is_ok() {
            (self.wake)();
        }
    }

    fn every(&self) -> Duration {
        self.every
    }
}
