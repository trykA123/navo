use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

use crate::pipe::{
    CancelToken, Cancellation, PipeError, Query, Reply, Response, next_backoff,
    serve_instance_with, session::Transport,
};

struct Failing {
    failures: usize,
    accepts: AtomicUsize,
    cancel: CancelToken,
}

impl Transport for Failing {
    fn accept(&self, _: &Cancellation<'_>) -> Result<(), PipeError> {
        if self.accepts.fetch_add(1, Ordering::SeqCst) < self.failures {
            Err(PipeError::NotAvailable)
        } else {
            self.cancel.cancel();
            Err(PipeError::Cancelled)
        }
    }

    fn disconnect(&self) -> Result<(), PipeError> {
        Ok(())
    }

    fn read_query(&self, _: &Cancellation<'_>) -> Result<Query, PipeError> {
        Err(PipeError::NotAvailable)
    }

    fn write_response(&self, _: &Response, _: Instant) -> Result<(), PipeError> {
        Ok(())
    }
}

#[test]
fn an_accept_error_backs_off_and_the_loop_keeps_serving_until_cancelled() {
    let cancel = CancelToken::new();
    let pipe = Failing {
        failures: 3,
        accepts: AtomicUsize::new(0),
        cancel: cancel.clone(),
    };
    let started = Instant::now();
    let result = serve_instance_with(
        &pipe,
        &|_, _| Reply::default(),
        (&cancel, &CancelToken::new()),
        (Duration::from_millis(10), Duration::from_millis(40)),
    );
    assert!(result.is_ok());
    assert_eq!(pipe.accepts.load(Ordering::SeqCst), 4);
    assert!(started.elapsed() >= Duration::from_millis(10 + 20 + 40));
}

#[test]
fn the_backoff_doubles_from_a_tenth_of_a_second_up_to_five_seconds() {
    let max = Duration::from_secs(5);
    let mut delay = Duration::from_millis(100);
    let mut seen = vec![delay];
    for _ in 0..7 {
        delay = next_backoff(delay, max);
        seen.push(delay);
    }
    assert_eq!(seen[1], Duration::from_millis(200));
    assert_eq!(seen[4], Duration::from_millis(1600));
    assert_eq!(seen[5], Duration::from_millis(3200));
    assert_eq!(seen[6], max);
}

struct LeavingClient {
    accepts: AtomicUsize,
    reads: AtomicUsize,
    handler_waiting: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    accepted_again: std::sync::mpsc::SyncSender<()>,
}

impl Transport for LeavingClient {
    fn accept(&self, _: &Cancellation<'_>) -> Result<(), PipeError> {
        if self.accepts.fetch_add(1, Ordering::SeqCst) == 0 {
            return Ok(());
        }
        let _ = self.accepted_again.try_send(());
        Err(PipeError::Cancelled)
    }

    fn disconnect(&self) -> Result<(), PipeError> {
        Ok(())
    }

    fn read_query(&self, _: &Cancellation<'_>) -> Result<Query, PipeError> {
        if self.reads.fetch_add(1, Ordering::SeqCst) == 0 {
            return Ok(Query {
                request: crate::pipe::Request { terms: Vec::new() },
                generation: 1,
            });
        }
        let _ = self.handler_waiting.lock().unwrap().recv();
        Err(PipeError::NotAvailable)
    }

    fn write_response(&self, _: &Response, _: Instant) -> Result<(), PipeError> {
        Ok(())
    }
}

#[test]
fn an_instance_accepts_again_within_50ms_after_its_client_leaves_while_the_handler_waits_on_the_index_lock()
 {
    let index = std::sync::RwLock::new(());
    let (waiting, handler_waiting) = std::sync::mpsc::sync_channel(1);
    let (accepted_again, accepted) = std::sync::mpsc::sync_channel(1);
    let pipe = LeavingClient {
        accepts: AtomicUsize::new(0),
        reads: AtomicUsize::new(0),
        handler_waiting: std::sync::Mutex::new(handler_waiting),
        accepted_again,
    };
    let handler = |_: &crate::pipe::Request, cancel: &CancelToken| {
        if !cancel.is_cancelled() {
            let _ = waiting.try_send(());
            let _index = index.read().unwrap();
        }
        Reply::default()
    };
    let batch = index.write().unwrap();
    let in_time = std::thread::scope(|scope| {
        let server = scope.spawn(|| {
            serve_instance_with(
                &pipe,
                &handler,
                (&CancelToken::new(), &CancelToken::new()),
                (Duration::from_millis(10), Duration::from_millis(40)),
            )
        });
        let in_time = accepted.recv_timeout(Duration::from_millis(50)).is_ok();
        drop(batch);
        server.join().unwrap().unwrap();
        in_time
    });
    assert!(
        in_time,
        "the instance stays busy until the index write lock is released"
    );
}

struct TwoClients {
    accepts: AtomicUsize,
    reads: AtomicUsize,
    written: std::sync::Mutex<Vec<Response>>,
    status_sent: std::sync::mpsc::SyncSender<()>,
    status_seen: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}

impl Transport for TwoClients {
    fn accept(&self, _: &Cancellation<'_>) -> Result<(), PipeError> {
        if self.accepts.fetch_add(1, Ordering::SeqCst) < 2 {
            Ok(())
        } else {
            Err(PipeError::Cancelled)
        }
    }

    fn disconnect(&self) -> Result<(), PipeError> {
        Ok(())
    }

    fn read_query(&self, _: &Cancellation<'_>) -> Result<Query, PipeError> {
        let read = self.reads.fetch_add(1, Ordering::SeqCst);
        if read.is_multiple_of(2) {
            return Ok(Query {
                request: crate::pipe::Request { terms: Vec::new() },
                generation: read as u64 + 1,
            });
        }
        let _ = self
            .status_seen
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5));
        Err(PipeError::NotAvailable)
    }

    fn write_response(&self, response: &Response, _: Instant) -> Result<(), PipeError> {
        if matches!(response, Response::Status { .. }) {
            let _ = self.status_sent.try_send(());
        }
        self.written.lock().unwrap().push(response.clone());
        Ok(())
    }
}

#[test]
fn a_panicking_handler_answers_with_an_error_and_the_instance_keeps_serving() {
    let (status_sent, status_seen) = std::sync::mpsc::sync_channel(2);
    let pipe = TwoClients {
        accepts: AtomicUsize::new(0),
        reads: AtomicUsize::new(0),
        written: std::sync::Mutex::new(Vec::new()),
        status_sent,
        status_seen: std::sync::Mutex::new(status_seen),
    };
    let calls = AtomicUsize::new(0);
    let handler = |_: &crate::pipe::Request, _: &CancelToken| {
        assert!(
            calls.fetch_add(1, Ordering::SeqCst) > 0,
            "the first search panics"
        );
        Reply::default()
    };
    let result = serve_instance_with(
        &pipe,
        &handler,
        (&CancelToken::new(), &CancelToken::new()),
        (Duration::from_millis(10), Duration::from_millis(40)),
    );
    assert!(result.is_ok());
    let errors: Vec<_> = pipe
        .written
        .lock()
        .unwrap()
        .iter()
        .filter_map(|response| match response {
            Response::Status { status, .. } => Some(
                status
                    .volumes
                    .iter()
                    .flat_map(|volume| volume.errors.clone())
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 2);
    assert_eq!(errors[0], ["index search failed"]);
    assert!(errors[1].is_empty());
}

struct Endless {
    accepts: AtomicUsize,
}

impl Transport for Endless {
    fn accept(&self, _: &Cancellation<'_>) -> Result<(), PipeError> {
        if self.accepts.fetch_add(1, Ordering::SeqCst) < 50 {
            Ok(())
        } else {
            Err(PipeError::Cancelled)
        }
    }

    fn disconnect(&self) -> Result<(), PipeError> {
        Ok(())
    }

    fn read_query(&self, _: &Cancellation<'_>) -> Result<Query, PipeError> {
        Err(PipeError::NotAvailable)
    }

    fn write_response(&self, _: &Response, _: Instant) -> Result<(), PipeError> {
        Ok(())
    }
}

#[test]
fn an_instance_stops_accepting_once_its_answer_thread_is_gone() {
    let pipe = Endless {
        accepts: AtomicUsize::new(0),
    };
    let (answers, _) = std::sync::mpsc::channel();
    let result = crate::pipe::accept_sessions(
        &pipe,
        (&answers, &|| pipe.accepts.load(Ordering::SeqCst) > 0),
        &[&CancelToken::new()],
        (Duration::from_millis(10), Duration::from_millis(40)),
    );
    assert!(matches!(result, Err(PipeError::NotAvailable)));
    assert_eq!(pipe.accepts.load(Ordering::SeqCst), 1);
}
