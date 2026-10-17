mod known;
mod look;
mod patch;
mod recount;
mod session;

use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use rusty_core::Location;

use super::PROGRESS_EVERY;
use super::profile::FolderProfile;
use crate::{CancelToken, IoQueue, Provider};
use session::{Note, Parts, Session};

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
pub struct LiveHandle {
    notes: Sender<Note>,
    cancel: CancelToken,
}

impl LiveHandle {
    pub fn recount(&self) {
        let _ = self.notes.send(Note::Recount);
    }
}

impl Drop for LiveHandle {
    fn drop(&mut self) {
        self.cancel.cancel();
        let _ = self.notes.send(Note::Stop);
    }
}

pub fn spawn_live(
    queue: &Arc<IoQueue>,
    request: LiveRequest,
    sink: impl LiveSink + 'static,
) -> LiveHandle {
    let (notes, receive) = mpsc::channel();
    let cancel = CancelToken::new();
    let session = Session::new(Parts {
        provider: request.provider,
        folder: request.folder,
        base: request.base,
        queue: Arc::clone(queue),
        sink,
        notes: notes.clone(),
        cancel: cancel.clone(),
    });
    let _ = std::thread::Builder::new()
        .name("folder-live".to_owned())
        .spawn(move || session.run(&receive));
    LiveHandle { notes, cancel }
}

#[cfg(test)]
mod tests;
