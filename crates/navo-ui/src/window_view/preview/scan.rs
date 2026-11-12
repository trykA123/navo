use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use navo_fs::{FolderProfile, LiveHandle, LiveSink, LiveSnapshot, ProfileState};

use super::inbox::Arrival;
use crate::browser::Waker;

pub(in crate::window_view) enum Phase {
    Counting { started: Instant },
    Done { took: Option<Duration> },
    Stopped,
    Failed,
}

pub(in crate::window_view) struct Scan {
    pub profile: Option<FolderProfile>,
    pub phase: Phase,
    pub is_recounting: bool,
    is_live: bool,
    session: Option<LiveHandle>,
    run: u64,
}

impl Scan {
    pub(super) fn cached(profile: FolderProfile, session: LiveHandle, run: u64) -> Self {
        Self {
            profile: Some(profile),
            phase: Phase::Done { took: None },
            is_recounting: false,
            is_live: false,
            session: Some(session),
            run,
        }
    }

    pub(super) fn counting(session: LiveHandle, run: u64) -> Self {
        Self {
            profile: None,
            phase: Phase::Counting {
                started: Instant::now(),
            },
            is_recounting: false,
            is_live: false,
            session: Some(session),
            run,
        }
    }

    /// Waits for index totals without a profile or a live session, so no walk starts.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "T14 starts previews with this placeholder")
    )]
    pub(super) fn waiting(run: u64) -> Self {
        Self {
            profile: None,
            phase: Phase::Stopped,
            is_recounting: false,
            is_live: false,
            session: None,
            run,
        }
    }

    pub(super) fn resume(&mut self, session: LiveHandle, run: u64) {
        self.phase = Phase::Counting {
            started: Instant::now(),
        };
        self.session = Some(session);
        self.run = run;
    }

    pub(super) fn is_counting(&self) -> bool {
        matches!(self.phase, Phase::Counting { .. })
    }

    pub(super) fn is_live(&self) -> bool {
        self.session.is_some() && self.is_live
    }

    pub(super) fn stop(&mut self) -> bool {
        if !self.is_counting() {
            return false;
        }
        self.end();
        true
    }

    pub(super) fn end(&mut self) -> bool {
        if self.is_counting() {
            self.phase = Phase::Stopped;
        }
        self.is_live = false;
        self.is_recounting = false;
        self.session.take().is_some()
    }

    pub(super) fn take_session(&mut self) -> Option<Self> {
        self.session.as_ref()?;
        let left = Self {
            profile: None,
            phase: Phase::Stopped,
            is_recounting: false,
            is_live: false,
            session: None,
            run: self.run,
        };
        Some(std::mem::replace(self, left))
    }

    pub(super) fn recount(&self) -> bool {
        self.session.as_ref().map(LiveHandle::recount).is_some()
    }

    pub(super) fn accept(&mut self, run: u64, snapshot: LiveSnapshot) -> bool {
        if run != self.run || self.session.is_none() {
            return false;
        }
        let took = match &self.phase {
            Phase::Counting { started } => Some(started.elapsed()),
            _ => None,
        };
        let next = match snapshot.profile.state {
            ProfileState::Running => None,
            ProfileState::Done if took.is_some() => Some(Phase::Done { took }),
            ProfileState::Done => None,
            ProfileState::Stopped => Some(Phase::Stopped),
            ProfileState::Failed(_) => Some(Phase::Failed),
        };
        if let Some(next) = next {
            self.phase = next;
        }
        self.is_live = snapshot.is_live;
        self.is_recounting = snapshot.is_recounting;
        self.profile = Some(snapshot.profile);
        true
    }
}

pub(super) struct Snapshots {
    pub send: Sender<Arrival>,
    pub wake: Waker,
    pub run: u64,
    pub every: Duration,
}

impl LiveSink for Snapshots {
    fn snapshot(&mut self, live: &LiveSnapshot) {
        let arrival = Arrival::Snapshot {
            run: self.run,
            snapshot: live.clone(),
        };
        if self.send.send(arrival).is_ok() {
            (self.wake)();
        }
    }

    fn progress_every(&self) -> Duration {
        self.every
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_opens_no_session_or_profile() {
        let mut scan = Scan::waiting(7);
        assert!(
            scan.session.is_none(),
            "waiting must not open a live session"
        );
        assert!(scan.profile.is_none(), "waiting must not carry a profile");
        assert!(!scan.is_live(), "waiting is not a live scan");
        assert!(!scan.is_recounting, "waiting is not recounting");
        assert!(
            scan.take_session().is_none(),
            "waiting has no session to hand over"
        );
        assert!(!scan.stop(), "waiting has no walk to stop");
    }

    #[test]
    fn waiting_is_stopped_and_keeps_the_run() {
        let scan = Scan::waiting(7);
        assert!(
            matches!(scan.phase, Phase::Stopped),
            "waiting must not be counting"
        );
        assert!(
            !scan.is_counting(),
            "waiting must not look like a running scan"
        );
        assert_eq!(scan.run, 7, "totals arrivals match the supplied run");
    }
}
