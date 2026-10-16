use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

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
        if !station.pump(wait) {
            return;
        }
    }
}

pub(super) fn mark(busy: &BusySince, is_busy: bool) {
    *busy.lock().unwrap_or_else(PoisonError::into_inner) = is_busy.then(Instant::now);
}

#[cfg(test)]
mod tests {
    use std::sync::PoisonError;
    use std::sync::mpsc::{self, Sender};
    use std::time::Duration;

    use super::{BusySince, Station, serve};

    enum Request {
        Fill,
        Invoke,
        Shutdown,
    }

    struct FakeStation {
        requests: Sender<Request>,
        busy: BusySince,
        pumps_left: Option<usize>,
        pumped: usize,
        log: Vec<&'static str>,
        busy_while_pumping: bool,
    }

    impl Station for FakeStation {
        type Request = Request;

        fn can_hang(request: &Request) -> bool {
            matches!(request, Request::Fill)
        }

        fn handle(&mut self, request: Request) -> bool {
            match request {
                Request::Fill => {
                    self.pumps_left = Some(3);
                    self.log.push("fill asked");
                }
                Request::Invoke if self.pumps_left.is_some() => {
                    self.log.push("invoke while loading")
                }
                Request::Invoke => self.log.push("invoke"),
                Request::Shutdown => return false,
            }
            true
        }

        fn settle_fills(&mut self) -> Option<Duration> {
            if self.pumps_left == Some(0) {
                self.pumps_left = None;
                self.log.push("filled");
                self.requests.send(Request::Shutdown).unwrap();
            }
            self.pumps_left.map(|_| Duration::from_millis(10))
        }

        fn pump(&mut self, timeout: Option<Duration>) -> bool {
            let is_busy = self
                .busy
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_some();
            self.busy_while_pumping |= is_busy;
            if timeout.is_none() {
                return false;
            }
            self.pumped += 1;
            if self.pumped == 1 {
                self.requests.send(Request::Invoke).unwrap();
            }
            if let Some(left) = self.pumps_left.as_mut() {
                *left -= 1;
            }
            true
        }
    }

    fn fake() -> (FakeStation, mpsc::Receiver<Request>) {
        let (requests, receiver) = mpsc::channel();
        let station = FakeStation {
            requests,
            busy: BusySince::default(),
            pumps_left: None,
            pumped: 0,
            log: Vec::new(),
            busy_while_pumping: false,
        };
        (station, receiver)
    }

    #[test]
    fn other_requests_are_served_while_a_submenu_fills() {
        let (mut station, receiver) = fake();
        station.requests.send(Request::Fill).unwrap();
        let busy = station.busy.clone();
        serve(&mut station, &receiver, &busy);
        assert_eq!(
            station.log,
            ["fill asked", "invoke while loading", "filled"]
        );
    }

    #[test]
    fn a_background_fill_does_not_count_as_busy() {
        let (mut station, receiver) = fake();
        station.requests.send(Request::Fill).unwrap();
        let busy = station.busy.clone();
        serve(&mut station, &receiver, &busy);
        assert!(station.pumped >= 3);
        assert!(!station.busy_while_pumping);
    }

    #[test]
    fn with_no_fills_the_loop_waits_without_a_timeout() {
        let (mut station, receiver) = fake();
        station.requests.send(Request::Invoke).unwrap();
        let busy = station.busy.clone();
        serve(&mut station, &receiver, &busy);
        assert_eq!(station.log, ["invoke"]);
        assert_eq!(station.pumped, 0);
    }
}
