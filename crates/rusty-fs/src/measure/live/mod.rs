mod recount;

use std::sync::Arc;
use std::time::Duration;

use rusty_core::Location;

use super::PROGRESS_EVERY;
use super::profile::FolderProfile;
use crate::{IoQueue, Provider};

pub const LIVE_EVERY: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveSnapshot {
    pub profile: FolderProfile,
    pub is_live: bool,
    pub is_recounting: bool,
    pub counts: u64,
    pub changes: u64,
}

pub trait LiveSink: Send {
    fn snapshot(&mut self, live: &LiveSnapshot);

    fn progress_every(&self) -> Duration {
        PROGRESS_EVERY
    }

    fn refresh_every(&self) -> Duration {
        LIVE_EVERY
    }
}

pub struct LiveRequest {
    pub provider: Arc<dyn Provider>,
    pub folder: Location,
    pub base: Option<FolderProfile>,
}

#[must_use = "dropping the handle stops the live profile and its watch"]
pub struct LiveHandle {}

impl LiveHandle {
    pub fn recount(&self) {
        todo!()
    }
}

pub fn spawn_live(
    _queue: &Arc<IoQueue>,
    _request: LiveRequest,
    _sink: impl LiveSink + 'static,
) -> LiveHandle {
    todo!()
}

#[cfg(test)]
mod tests;
