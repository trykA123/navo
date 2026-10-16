use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

pub(super) const HUNG_AFTER: Duration = Duration::from_secs(2);

pub(super) type BusySince = Arc<Mutex<Option<Instant>>>;

pub(super) trait Station {
    type Request;

    fn can_hang(request: &Self::Request) -> bool;
    fn handle(&mut self, request: Self::Request) -> bool;
    fn settle_fills(&mut self) -> Option<Duration>;
    fn pump(&mut self, timeout: Option<Duration>) -> bool;
}

pub(super) fn serve<S: Station>(
    station: &mut S,
    requests: &Receiver<S::Request>,
    busy: &BusySince,
) {
    loop {
        while let Ok(request) = requests.try_recv() {
            mark(busy, S::can_hang(&request));
            let keep_running = station.handle(request);
            mark(busy, false);
            if !keep_running {
                return;
            }
        }
        let wait = station.settle_fills();
        mark(busy, wait.is_some());
        let keep_running = station.pump(wait);
        mark(busy, false);
        if !keep_running {
            return;
        }
    }
}

pub(super) fn is_hung(busy: &BusySince, now: Instant) -> bool {
    let busy = busy.lock().unwrap_or_else(PoisonError::into_inner);
    busy.is_some_and(|since| now.saturating_duration_since(since) > HUNG_AFTER)
}

pub(super) fn mark(busy: &BusySince, is_busy: bool) {
    *busy.lock().unwrap_or_else(PoisonError::into_inner) = is_busy.then(Instant::now);
}

#[cfg(test)]
mod tests;
