use navo_core::{Location, TabId};
use navo_fs::WatchEvent;

use super::operate::JobUpdate;
use super::{Loaded, cached, find, measure, naming, view_jobs};

pub(super) enum Message {
    Loaded(Loaded),
    CachedSorted(cached::CachedSorted),
    Viewed(view_jobs::Viewed),
    Measured(navo_fs::OwnerId, measure::Measured),
    Found(TabId, navo_fs::OwnerId, find::Update),
    Job(JobUpdate),
    Named(naming::Named),
    Watched(Location, WatchEvent),
    Recheck(Location),
    Prefetched(u64),
    Left(TabId, Vec<Location>),
}

impl Message {
    pub(super) fn is_queued(&self) -> bool {
        !matches!(
            self,
            Self::Job(
                JobUpdate::Progress { .. } | JobUpdate::Waiting { .. } | JobUpdate::Done { .. }
            ) | Self::Measured(..)
                | Self::Found(..)
                | Self::Watched(..)
                | Self::Recheck(_)
                | Self::Prefetched(_)
                | Self::CachedSorted(_)
                | Self::Viewed(_)
        )
    }
}
