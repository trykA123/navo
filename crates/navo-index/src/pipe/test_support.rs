#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test-support fixtures fail immediately on invalid data or broken coordination"
)]

use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use super::session::Transport;
use super::{CancelToken, Cancellation, PipeError, Query, Reply, Request, Response, frames};

const WAIT: Duration = Duration::from_secs(30);

struct FixturePipe {
    accepted: AtomicBool,
    queries: Mutex<mpsc::Receiver<Vec<u8>>>,
    responses: mpsc::Sender<Vec<u8>>,
}

impl Transport for FixturePipe {
    fn accept(&self, _: &Cancellation<'_>) -> Result<(), PipeError> {
        if self.accepted.swap(true, Ordering::Relaxed) {
            Err(PipeError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn disconnect(&self) -> Result<(), PipeError> {
        Ok(())
    }

    fn read_query(&self, cancel: &Cancellation<'_>) -> Result<Query, PipeError> {
        while !cancel.is_cancelled() {
            match self
                .queries
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_millis(1))
            {
                Ok(frame) => return Ok(frames::decode(&frame)?),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return Err(PipeError::NotAvailable),
            }
        }
        Err(PipeError::Cancelled)
    }

    fn write_response(&self, response: &Response, _: Instant) -> Result<(), PipeError> {
        self.responses
            .send(frames::encode(response)?)
            .map_err(|_| PipeError::NotAvailable)
    }
}

pub fn timed_answer(
    request: &Request,
    handler: &(impl Fn(&Request, &CancelToken) -> Reply + Sync),
) -> (Reply, Duration) {
    let (queries, incoming) = mpsc::channel();
    let (responses, received) = mpsc::channel();
    let pipe = FixturePipe {
        accepted: AtomicBool::new(false),
        queries: Mutex::new(incoming),
        responses,
    };
    let cancel = CancelToken::new();
    std::thread::scope(|scope| {
        let _stop = super::session::StopOnDrop(cancel.clone());
        let started = Instant::now();
        let server = scope.spawn(|| {
            super::serve_instance_with(
                &pipe,
                handler,
                (&cancel, &CancelToken::new()),
                (Duration::ZERO, Duration::ZERO),
            )
        });
        queries
            .send(
                frames::encode(&Query {
                    request: request.clone(),
                    generation: 1,
                })
                .unwrap(),
            )
            .unwrap();
        let result = receive(&received);
        let elapsed = started.elapsed();
        cancel.cancel();
        server.join().unwrap().unwrap();
        (result, elapsed)
    })
}

fn receive(received: &mpsc::Receiver<Vec<u8>>) -> Reply {
    let mut reply = Reply::default();
    let deadline = Instant::now() + WAIT;
    for status in [false, true] {
        let bytes = received
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
        match frames::decode(&bytes).unwrap() {
            Response::Hits {
                generation: 1,
                hits,
            } if !status => reply.hits = hits,
            Response::Status {
                generation: 1,
                status: found,
            } if status => reply.status = found,
            response => panic!("unexpected timing response: {response:?}"),
        }
    }
    reply
}
