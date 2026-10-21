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
