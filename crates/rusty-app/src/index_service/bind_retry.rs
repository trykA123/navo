use rusty_index::pipe::CancelToken;
use std::time::{Duration, Instant};

pub(super) fn retry_delay(attempt: usize) -> Duration {
    Duration::from_secs([1, 5, 30, 60][attempt.min(3)])
}

pub(super) fn bind_with_retry<T, E: std::fmt::Display>(
    cancel: &CancelToken,
    mut bind: impl FnMut() -> Result<T, E>,
    mut report: impl FnMut(String),
    mut wait: impl FnMut(Duration),
) -> Option<T> {
    let mut attempt = 0;
    while !cancel.is_cancelled() {
        match bind() {
            Ok(server) => return Some(server),
            Err(error) => report(format!("service pipe bind: {error}")),
        }
        if cancel.is_cancelled() {
            break;
        }
        wait(retry_delay(attempt));
        attempt = attempt.saturating_add(1);
    }
    None
}

pub(super) fn wait_for_retry(cancel: &CancelToken, delay: Duration) {
    let started = Instant::now();
    while !cancel.is_cancelled() {
        let Some(remaining) = delay.checked_sub(started.elapsed()) else {
            break;
        };
        std::thread::park_timeout(remaining.min(Duration::from_millis(50)));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn cancelled_retry_wait_returns_without_waiting() {
        let cancel = rusty_index::pipe::CancelToken::new();
        cancel.cancel();
        super::wait_for_retry(&cancel, std::time::Duration::MAX);
    }
}
