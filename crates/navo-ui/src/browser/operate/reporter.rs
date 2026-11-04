use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use navo_fs::Progress;

use super::JobUpdate;
use super::stream::StreamPaste;
use crate::browser::{Message, Waker};

const PROGRESS_EVERY: Duration = Duration::from_millis(50);

type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

/// Sends a running job's progress to the browser, at most every [`PROGRESS_EVERY`] unless
/// pausing changes.
pub(super) struct Reporter<'a> {
    id: u64,
    stream: Option<&'a StreamPaste>,
    sender: &'a Sender<Message>,
    wake: &'a Waker,
    sent: Instant,
    now: Clock,
    last: Option<Progress>,
}

impl<'a> Reporter<'a> {
    pub(super) fn new(
        id: u64,
        stream: Option<&'a StreamPaste>,
        (sender, wake): (&'a Sender<Message>, &'a Waker),
    ) -> Self {
        Self::with_clock(id, stream, (sender, wake), Arc::new(Instant::now))
    }

    fn with_clock(
        id: u64,
        stream: Option<&'a StreamPaste>,
        (sender, wake): (&'a Sender<Message>, &'a Waker),
        now: Clock,
    ) -> Self {
        Self {
            id,
            stream,
            sender,
            wake,
            sent: now(),
            now,
            last: None,
        }
    }

    /// The last progress seen, as shown.
    pub(super) fn into_last(self) -> Option<Progress> {
        self.last
    }

    pub(super) fn report(&mut self, progress: &Progress) {
        let was_paused = self.last.as_ref().is_some_and(|last| last.paused);
        let items_only = self.stream.is_some_and(|stream| stream.items_only);
        self.last = Some(Progress {
            bytes_done: if items_only { 0 } else { progress.bytes_done },
            bytes_total: if items_only { 0 } else { progress.bytes_total },
            files_done: progress.files_done,
            files_total: progress.files_total,
            current: None,
            paused: progress.paused,
        });
        let now = (self.now)();
        if progress.paused == was_paused
            && now.saturating_duration_since(self.sent) < PROGRESS_EVERY
        {
            return;
        }
        let progress = self
            .stream
            .map_or_else(|| progress.clone(), |stream| stream.shown(progress));
        self.last = Some(progress.clone());
        self.sent = now;
        let id = self.id;
        let _ = self
            .sender
            .send(Message::Job(JobUpdate::Progress { id, progress }));
        (self.wake)();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::mpsc;
    use std::time::Duration;

    use navo_fs::Progress;

    use super::*;
    use crate::browser::Waker;
    use crate::browser::operate::StreamPaste;

    fn progress(bytes: u64, paused: bool) -> Progress {
        Progress {
            bytes_done: bytes,
            bytes_total: 1_000,
            files_done: bytes,
            files_total: 1_000,
            current: None,
            paused,
        }
    }

    fn sent(receiver: &mpsc::Receiver<Message>) -> Vec<u64> {
        let mut found = Vec::new();
        while let Ok(Message::Job(JobUpdate::Progress { progress, .. })) = receiver.try_recv() {
            found.push(progress.bytes_done);
        }
        found
    }

    #[test]
    fn items_only_paste_keeps_zero_bytes_in_the_final_snapshot() {
        let (sender, receiver) = mpsc::channel();
        let wake: Waker = Arc::new(|| {});
        let stream = StreamPaste {
            sources: Default::default(),
            items: Vec::new(),
            refused: Vec::new(),
            items_only: true,
            explain: None,
        };
        let mut reporter = Reporter::new(1, Some(&stream), (&sender, &wake));
        std::thread::sleep(Duration::from_millis(60));
        reporter.report(&progress(1_000, false));
        reporter.report(&progress(2_000, false));
        assert_eq!(sent(&receiver), [0]);
        assert_eq!(reporter.into_last().unwrap().bytes_done, 0);
    }

    #[test]
    fn throttles_before_cloning_and_still_sends_the_last_reported_progress() {
        use std::sync::atomic::{AtomicU64, Ordering};

        let (sender, receiver) = mpsc::channel();
        let wake: Waker = Arc::new(|| {});
        let base = Instant::now();
        let offset = Arc::new(AtomicU64::new(0));
        let clock = offset.clone();
        let now: Clock =
            Arc::new(move || base + Duration::from_millis(clock.load(Ordering::SeqCst)));
        let mut reporter = Reporter::with_clock(1, None, (&sender, &wake), now);
        offset.store(60, Ordering::SeqCst);
        for bytes in 1..=200_000 {
            reporter.report(&progress(bytes, false));
        }
        assert_eq!(sent(&receiver), [1]);

        offset.store(120, Ordering::SeqCst);
        reporter.report(&progress(200_001, false));
        reporter.report(&progress(200_002, true));
        assert_eq!(sent(&receiver), [200_001, 200_002]);
        assert_eq!(reporter.into_last().unwrap().bytes_done, 200_002);
    }
}
