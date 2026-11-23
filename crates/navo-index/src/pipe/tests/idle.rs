//! Idle limit on server pipe connections: a client that sends nothing is dropped after the
//! limit, and the instance then serves the next client.

use std::{
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use crate::pipe::{
    CancelToken, Cancellation, PipeError, Query, Reply, Request, Response, serve_instance_with,
    session::{IDLE, Transport},
};

/// The shortened silence limit the fake reports, in place of the real 30 s.
const TEST_IDLE: Duration = Duration::from_millis(50);
/// How long the silent fake client waits before it gives up by itself (the red case).
const GIVE_UP: Duration = Duration::from_secs(2);

/// A fake instance: client 1 connects and sends nothing; client 2 sends one search and leaves
/// after its answer; a third accept stops the loop.
struct SilentThenServed {
    accepts: AtomicUsize,
    reads: AtomicUsize,
    disconnects: AtomicUsize,
    dropped_after: Mutex<Option<Duration>>,
    written: Mutex<Vec<Response>>,
    answered: mpsc::SyncSender<()>,
    answer_seen: Mutex<mpsc::Receiver<()>>,
}

impl SilentThenServed {
    /// Waits like a silent client: `TimedOut` once the server's deadline passes, else the client
    /// leaves by itself after `GIVE_UP` (`NotAvailable`).
    fn stay_silent(
        &self,
        deadline: Option<Instant>,
        cancel: &Cancellation<'_>,
    ) -> Result<Query, PipeError> {
        let connected = Instant::now();
        while !cancel.is_cancelled() && connected.elapsed() < GIVE_UP {
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                *self.dropped_after.lock().unwrap() = Some(connected.elapsed());
                return Err(PipeError::TimedOut);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Err(PipeError::NotAvailable)
    }
}

impl Transport for SilentThenServed {
    fn accept(&self, _: &Cancellation<'_>) -> Result<(), PipeError> {
        if self.accepts.fetch_add(1, Ordering::SeqCst) < 2 {
            Ok(())
        } else {
            Err(PipeError::Cancelled)
        }
    }

    fn disconnect(&self) -> Result<(), PipeError> {
        self.disconnects.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn read_query(
        &self,
        deadline: Option<Instant>,
        cancel: &Cancellation<'_>,
    ) -> Result<Query, PipeError> {
        match self.reads.fetch_add(1, Ordering::SeqCst) {
            0 => self.stay_silent(deadline, cancel),
            1 => Ok(Query {
                request: Request { terms: Vec::new() },
                generation: 7,
            }),
            _ => {
                let _ = self
                    .answer_seen
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5));
                Err(PipeError::NotAvailable)
            }
        }
    }

    fn write_response(&self, response: &Response, _: Instant) -> Result<(), PipeError> {
        self.written.lock().unwrap().push(response.clone());
        if matches!(response, Response::Status { .. }) {
            let _ = self.answered.try_send(());
        }
        Ok(())
    }

    fn idle_limit(&self) -> Duration {
        TEST_IDLE
    }
}

#[test]
fn a_silent_client_is_dropped_after_the_idle_limit_and_the_next_client_is_served() {
    let (answered, answer_seen) = mpsc::sync_channel(1);
    let pipe = SilentThenServed {
        accepts: AtomicUsize::new(0),
        reads: AtomicUsize::new(0),
        disconnects: AtomicUsize::new(0),
        dropped_after: Mutex::new(None),
        written: Mutex::new(Vec::new()),
        answered,
        answer_seen: Mutex::new(answer_seen),
    };
    let handler = |_: &Request, _: &CancelToken| Reply::default();
    let result = serve_instance_with(
        &pipe,
        &handler,
        (&CancelToken::new(), &CancelToken::new()),
        (Duration::from_millis(10), Duration::from_millis(40)),
    );
    assert!(
        result.is_ok(),
        "a dropped client must not stop the instance"
    );
    let dropped_after = *pipe.dropped_after.lock().unwrap();
    assert!(
        dropped_after.is_some_and(|after| after >= TEST_IDLE && after < GIVE_UP / 2),
        "the server must drop the silent client at its idle limit, got {dropped_after:?}"
    );
    assert_eq!(pipe.disconnects.load(Ordering::SeqCst), 2);
    let written = pipe.written.lock().unwrap();
    assert!(
        matches!(
            written.as_slice(),
            [
                Response::Hits { generation: 7, .. },
                Response::Status { generation: 7, .. }
            ]
        ),
        "the client after the silent one gets its answer, got {written:?}"
    );
    assert_eq!(IDLE, Duration::from_secs(30));
}
