use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Event {
    Open(u32),
    Release,
    Stop,
    Wait,
}

struct Fake {
    script: Vec<Status>,
    at: usize,
    events: Vec<Event>,
    clock: Duration,
    accepts_stop: Vec<bool>,
    exits: bool,
}

fn status(state: State, process: u32) -> Status {
    Status { state, process }
}

impl Fake {
    fn new(script: &[Status]) -> Self {
        Self {
            script: script.to_vec(),
            at: 0,
            events: Vec::new(),
            clock: Duration::ZERO,
            accepts_stop: Vec::new(),
            exits: true,
        }
    }
}

impl Control for Fake {
    fn status(&mut self) -> io::Result<Status> {
        let index = self.at.min(self.script.len() - 1);
        self.at += 1;
        Ok(self.script[index])
    }
    fn open(&mut self, process: u32) -> io::Result<()> {
        self.events.push(Event::Open(process));
        Ok(())
    }
    fn release(&mut self) {
        self.events.push(Event::Release);
    }
    fn stop(&mut self) -> io::Result<bool> {
        self.events.push(Event::Stop);
        Ok(if self.accepts_stop.is_empty() {
            true
        } else {
            self.accepts_stop.remove(0)
        })
    }
    fn wait(&mut self, limit: Duration) -> io::Result<bool> {
        self.events.push(Event::Wait);
        assert_eq!(limit, PROCESS_WAIT);
        Ok(self.exits)
    }
    fn elapsed(&self) -> Duration {
        self.clock
    }
    fn pause(&mut self) {
        self.clock += POLL;
    }
}

const LIMITS: StopLimits = StopLimits::FULL;

#[test]
fn removal_of_a_running_or_paused_service_retains_its_process_then_stops_and_waits() {
    for state in [
        State::Running,
        State::Paused,
        State::PausePending,
        State::ContinuePending,
    ] {
        let mut fake = Fake::new(&[
            status(state, 40),
            status(state, 40),
            status(State::StopPending, 40),
            status(State::Stopped, 0),
        ]);
        stop(&mut fake, LIMITS).unwrap();
        assert_eq!(
            fake.events,
            [Event::Open(40), Event::Stop, Event::Wait],
            "{state:?}"
        );
    }
}

#[test]
fn removal_polls_a_starting_service_until_it_runs_before_stopping_it() {
    let mut fake = Fake::new(&[
        status(State::StartPending, 7),
        status(State::StartPending, 7),
        status(State::Running, 7),
        status(State::Running, 7),
        status(State::Stopped, 0),
    ]);
    stop(&mut fake, LIMITS).unwrap();
    assert_eq!(fake.events, [Event::Open(7), Event::Stop, Event::Wait]);
}

#[test]
fn removal_of_a_stopping_service_retains_a_known_process_without_a_second_stop() {
    let mut fake = Fake::new(&[
        status(State::StopPending, 9),
        status(State::StopPending, 9),
        status(State::Stopped, 0),
    ]);
    stop(&mut fake, LIMITS).unwrap();
    assert_eq!(fake.events, [Event::Open(9), Event::Wait]);
}

#[test]
fn removal_of_a_stopping_service_without_a_process_id_only_polls() {
    let mut fake = Fake::new(&[
        status(State::StopPending, 0),
        status(State::StopPending, 0),
        status(State::Stopped, 0),
    ]);
    stop(&mut fake, LIMITS).unwrap();
    assert!(fake.events.is_empty(), "{:?}", fake.events);
}

#[test]
fn removal_of_a_stopped_service_takes_no_handle_and_does_not_wait() {
    let mut fake = Fake::new(&[status(State::Stopped, 0)]);
    stop(&mut fake, LIMITS).unwrap();
    assert!(fake.events.is_empty(), "{:?}", fake.events);
}

#[test]
fn removal_releases_a_handle_when_the_service_reports_another_process() {
    let mut fake = Fake::new(&[
        status(State::Running, 5),
        status(State::Running, 6),
        status(State::Running, 6),
        status(State::Running, 6),
        status(State::Stopped, 0),
    ]);
    stop(&mut fake, LIMITS).unwrap();
    assert_eq!(
        fake.events,
        [
            Event::Open(5),
            Event::Release,
            Event::Open(6),
            Event::Stop,
            Event::Wait
        ]
    );
}

#[test]
fn removal_resends_a_stop_the_service_could_not_accept_yet() {
    let mut fake = Fake::new(&[
        status(State::PausePending, 3),
        status(State::PausePending, 3),
        status(State::Paused, 3),
        status(State::Stopped, 0),
    ]);
    fake.accepts_stop = vec![false, true];
    stop(&mut fake, LIMITS).unwrap();
    assert_eq!(
        fake.events,
        [Event::Open(3), Event::Stop, Event::Stop, Event::Wait]
    );
}

#[test]
fn removal_fails_with_a_timeout_when_the_service_does_not_stop_within_12_seconds() {
    let mut fake = Fake::new(&[status(State::Running, 4)]);
    let error = stop(&mut fake, LIMITS).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(fake.clock >= STOP_LIMIT && fake.clock < STOP_LIMIT + POLL * 2);
    assert!(!fake.events.contains(&Event::Wait));
}

#[test]
fn removal_fails_with_a_timeout_when_the_stopped_process_does_not_exit() {
    let mut fake = Fake::new(&[
        status(State::Running, 4),
        status(State::Running, 4),
        status(State::Stopped, 0),
    ]);
    fake.exits = false;
    let error = stop(&mut fake, LIMITS).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
}

#[test]
fn removal_deletes_nothing_when_the_stop_times_out() {
    let mut fake = Fake::new(&[status(State::StopPending, 0)]);
    let mut cleaned = false;
    let result = super::super::run(
        || stop(&mut fake, LIMITS),
        || {
            cleaned = true;
            Ok(())
        },
    );
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
    assert!(!cleaned);
}

#[test]
fn removal_reads_every_service_state_and_refuses_an_unknown_one() {
    let states = [
        (1, State::Stopped),
        (2, State::StartPending),
        (3, State::StopPending),
        (4, State::Running),
        (5, State::ContinuePending),
        (6, State::PausePending),
        (7, State::Paused),
    ];
    for (raw, state) in states {
        assert_eq!(State::from_raw(raw).unwrap(), state);
    }
    assert!(State::from_raw(8).is_err());
}

#[test]
fn removal_limits_fit_the_deadline_inside_the_uninstall_budget() {
    assert!(STOP_LIMIT + PROCESS_WAIT < REMOVE_DEADLINE);
    assert!(REMOVE_DEADLINE < Duration::from_secs(25));
    let limits = StopLimits::within(Duration::from_secs(3));
    assert_eq!(limits.stop, Duration::from_secs(3));
    assert_eq!(limits.wait, Duration::from_secs(3));
}
