use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use navo_fs::Progress;

use super::JobUpdate;
use super::stream::StreamPaste;
use crate::browser::{Message, Waker};

const PROGRESS_EVERY: Duration = Duration::from_millis(50);

/// Sends a running job's progress to the browser, at most every [`PROGRESS_EVERY`] unless
/// pausing changes.
pub(super) struct Reporter<'a> {
    id: u64,
    stream: Option<&'a StreamPaste>,
    sender: &'a Sender<Message>,
    wake: &'a Waker,
    sent: Instant,
    last: Option<Progress>,
}

impl<'a> Reporter<'a> {
    pub(super) fn new(
        id: u64,
        stream: Option<&'a StreamPaste>,
        (sender, wake): (&'a Sender<Message>, &'a Waker),
    ) -> Self {
        Self {
            id,
            stream,
            sender,
            wake,
            sent: Instant::now(),
            last: None,
        }
    }

    /// The last progress seen, as shown.
    pub(super) fn into_last(self) -> Option<Progress> {
        self.last
    }

    pub(super) fn report(&mut self, progress: &Progress) {
        let was_paused = self.last.as_ref().is_some_and(|last| last.paused);
        let progress = &self
            .stream
            .map_or_else(|| progress.clone(), |stream| stream.shown(progress));
        self.last = Some(progress.clone());
        if progress.paused == was_paused && self.sent.elapsed() < PROGRESS_EVERY {
            return;
        }
        self.sent = Instant::now();
        let progress = progress.clone();
        let id = self.id;
        let _ = self
            .sender
            .send(Message::Job(JobUpdate::Progress { id, progress }));
        (self.wake)();
    }
}
