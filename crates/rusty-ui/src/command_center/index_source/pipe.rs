use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use rusty_index::pipe::{self, PipeError, Reply};

use crate::command_center::{Cancel, IndexOptions};

pub type PipeQuery =
    Arc<dyn Fn(&IndexOptions, &pipe::Query, &Cancel) -> Result<Reply, PipeError> + Send + Sync>;
const POLL: Duration = Duration::from_millis(5);

#[derive(Debug)]
pub enum PipeAnswer {
    Replied(Reply),
    Silent,
    Absent,
}

#[derive(Clone)]
pub struct PipeQueries {
    query: PipeQuery,
    busy: Arc<AtomicBool>,
}

struct Running(Arc<AtomicBool>);

impl Drop for Running {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

struct Stop(Cancel);

impl Drop for Stop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

impl PipeQueries {
    #[must_use]
    pub fn new(query: PipeQuery) -> Self {
        Self {
            query,
            busy: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(super) fn query(
        &self,
        options: IndexOptions,
        query: pipe::Query,
        cancel: &Cancel,
    ) -> PipeAnswer {
        let deadline = Instant::now() + pipe::CONNECT_TIMEOUT + pipe::QUERY_TIMEOUT;
        if cancel.is_cancelled() {
            return PipeAnswer::Silent;
        }
        while self.busy.swap(true, Ordering::AcqRel) {
            if cancel.is_cancelled() || Instant::now() >= deadline {
                return PipeAnswer::Silent;
            }
            std::thread::park_timeout(POLL);
        }
        let running = Running(self.busy.clone());
        let stopped = Stop(Cancel::new());
        let child = stopped.0.clone();
        let query_pipe = self.query.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("index-pipe-query".into())
            .spawn(move || {
                let _running = running;
                let _ = sender.send(query_pipe(&options, &query, &child));
            });
        if spawned.is_err() {
            return PipeAnswer::Silent;
        }
        while !cancel.is_cancelled() && Instant::now() < deadline {
            match receiver
                .recv_timeout(POLL.min(deadline.saturating_duration_since(Instant::now())))
            {
                Ok(Ok(reply)) => return PipeAnswer::Replied(reply),
                Ok(Err(PipeError::NotAvailable)) => return PipeAnswer::Absent,
                Ok(Err(_)) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return PipeAnswer::Silent;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
        PipeAnswer::Silent
    }
}
