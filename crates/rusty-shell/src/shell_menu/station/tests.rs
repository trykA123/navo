use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use super::{HUNG_AFTER, Health, IDLE_GRACE, Station, serve};

enum Request {
    Fill,
    Invoke,
    Shutdown,
}

fn is_marked(busy: &Health) -> bool {
    busy.marks().busy.is_some()
}

struct FakeStation {
    requests: Sender<Request>,
    busy: Health,
    pumps_left: Option<usize>,
    pumped: usize,
    log: Vec<&'static str>,
    busy_in_slices: Vec<bool>,
    busy_in_settles: Vec<bool>,
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
            Request::Invoke if self.pumps_left.is_some() => self.log.push("invoke while loading"),
            Request::Invoke => self.log.push("invoke"),
            Request::Shutdown => return false,
        }
        true
    }

    fn settle_fills(&mut self) -> Option<Duration> {
        self.busy_in_settles.push(is_marked(&self.busy));
        if self.pumps_left == Some(0) {
            self.pumps_left = None;
            self.log.push("filled");
            self.requests.send(Request::Shutdown).unwrap();
        }
        self.pumps_left.map(|_| Duration::from_millis(10))
    }

    fn pump(&mut self, timeout: Option<Duration>) -> bool {
        if timeout.is_none() {
            return false;
        }
        self.busy_in_slices.push(is_marked(&self.busy));
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

fn fake() -> (FakeStation, Receiver<Request>) {
    let (requests, receiver) = mpsc::channel();
    let station = FakeStation {
        requests,
        busy: Health::default(),
        pumps_left: None,
        pumped: 0,
        log: Vec::new(),
        busy_in_slices: Vec::new(),
        busy_in_settles: Vec::new(),
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
fn the_busy_mark_resets_after_every_background_slice() {
    let (mut station, receiver) = fake();
    station.requests.send(Request::Fill).unwrap();
    let busy = station.busy.clone();
    serve(&mut station, &receiver, &busy);
    assert_eq!(station.busy_in_slices, [true, true, true]);
    assert!(station.busy_in_settles.iter().all(|is_busy| !is_busy));
    assert!(!is_marked(&busy));
}

#[test]
fn with_no_fills_the_loop_waits_without_a_timeout() {
    let (mut station, receiver) = fake();
    station.requests.send(Request::Invoke).unwrap();
    let busy = station.busy.clone();
    serve(&mut station, &receiver, &busy);
    assert_eq!(station.log, ["invoke"]);
    assert_eq!(station.pumped, 0);
    assert!(!is_marked(&busy));
}

struct BlockingStation {
    is_loading: bool,
    entered: Sender<()>,
    release: Receiver<()>,
}

impl Station for BlockingStation {
    type Request = Request;

    fn can_hang(request: &Request) -> bool {
        matches!(request, Request::Fill)
    }

    fn handle(&mut self, request: Request) -> bool {
        self.is_loading |= matches!(request, Request::Fill);
        !matches!(request, Request::Shutdown)
    }

    fn settle_fills(&mut self) -> Option<Duration> {
        self.is_loading.then_some(Duration::from_millis(10))
    }

    fn pump(&mut self, timeout: Option<Duration>) -> bool {
        if timeout.is_none() {
            return false;
        }
        self.entered.send(()).unwrap();
        self.release.recv().unwrap();
        self.is_loading = false;
        true
    }
}

#[test]
fn a_dispatch_that_blocks_in_a_background_slice_is_seen_as_hung() {
    let (entered, entered_rx) = mpsc::channel();
    let (release_tx, release) = mpsc::channel();
    let (requests, receiver) = mpsc::channel();
    requests.send(Request::Fill).unwrap();
    let busy = Health::default();
    let serving = {
        let busy = busy.clone();
        std::thread::spawn(move || {
            let mut station = BlockingStation {
                is_loading: false,
                entered,
                release,
            };
            serve(&mut station, &receiver, &busy);
        })
    };
    entered_rx.recv().unwrap();
    let later = Instant::now() + HUNG_AFTER + Duration::from_millis(1);
    assert!(busy.is_hung(later));
    release_tx.send(()).unwrap();
    serving.join().unwrap();
    assert!(!busy.is_hung(later));
}

#[test]
fn a_short_busy_spell_is_not_hung() {
    let busy = Health::default();
    busy.mark(true);
    assert!(!busy.is_hung(Instant::now()));
}

struct IdleStation {
    requests: Sender<Request>,
    busy: Health,
    idle: Vec<&'static str>,
    log: Vec<&'static str>,
    hung_checks: Vec<(bool, bool)>,
}

impl Station for IdleStation {
    type Request = Request;

    fn can_hang(request: &Request) -> bool {
        matches!(request, Request::Fill)
    }

    fn handle(&mut self, request: Request) -> bool {
        self.log.push(match request {
            Request::Fill => "fill",
            Request::Invoke => "invoke",
            Request::Shutdown => return false,
        });
        true
    }

    fn settle_fills(&mut self) -> Option<Duration> {
        None
    }

    fn pump(&mut self, timeout: Option<Duration>) -> bool {
        timeout.is_some()
    }

    fn run_idle(&mut self) -> bool {
        if self.idle.is_empty() {
            return false;
        }
        let step = self.idle.remove(0);
        self.log.push(step);
        if step == "warm 1" {
            self.requests.send(Request::Fill).unwrap();
        }
        if step == "asked" {
            self.busy.note_waiting(Instant::now());
            self.requests.send(Request::Invoke).unwrap();
        }
        let now = Instant::now();
        let soon = now + HUNG_AFTER + Duration::from_millis(1);
        let much_later = now + HUNG_AFTER + IDLE_GRACE + Duration::from_millis(1);
        let checks = (self.busy.is_hung(soon), self.busy.is_hung(much_later));
        self.hung_checks.push(checks);
        true
    }
}

fn idle_station(steps: &[&'static str]) -> (IdleStation, Receiver<Request>) {
    let (requests, receiver) = mpsc::channel();
    let station = IdleStation {
        requests,
        busy: Health::default(),
        idle: steps.to_vec(),
        log: Vec::new(),
        hung_checks: Vec::new(),
    };
    (station, receiver)
}

#[test]
fn a_request_that_arrives_during_idle_work_runs_before_the_next_idle_step() {
    let (mut station, receiver) = idle_station(&["warm 1", "warm 2", "warm 3"]);
    let busy = station.busy.clone();
    serve(&mut station, &receiver, &busy);
    assert_eq!(station.log, ["warm 1", "fill", "warm 2", "warm 3"]);
}

#[test]
fn waiting_requests_run_before_any_idle_work() {
    let (mut station, receiver) = idle_station(&["warm 2"]);
    station.requests.send(Request::Invoke).unwrap();
    let busy = station.busy.clone();
    serve(&mut station, &receiver, &busy);
    assert_eq!(station.log, ["invoke", "warm 2"]);
}

#[test]
fn slow_idle_work_is_not_hung_until_its_grace_ends() {
    let (mut station, receiver) = idle_station(&["warm 1", "warm 2"]);
    let busy = station.busy.clone();
    serve(&mut station, &receiver, &busy);
    assert_eq!(station.hung_checks, [(false, true), (false, true)]);
    assert!(!is_marked(&busy));
}

#[test]
fn a_request_waiting_2_s_behind_idle_work_marks_it_hung() {
    let (mut station, receiver) = idle_station(&["asked", "warm 2"]);
    let busy = station.busy.clone();
    serve(&mut station, &receiver, &busy);
    assert_eq!(station.log, ["asked", "invoke", "warm 2"]);
    assert_eq!(station.hung_checks, [(true, true), (false, true)]);
}

#[test]
fn a_waiting_request_counts_only_behind_idle_work() {
    let busy = Health::default();
    let now = Instant::now();
    busy.note_waiting(now);
    let later = now + HUNG_AFTER + Duration::from_millis(1);
    assert!(!busy.is_hung(later));
    busy.mark_idle();
    assert!(busy.is_hung(later));
}
