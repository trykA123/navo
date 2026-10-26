use std::cell::Cell;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use rusty_core::placement::PhysicalRect;

use super::timing::{Event, Frame, Motion, Surface, drive};

fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

enum Cue {
    Send(Event),
    Hang,
}

struct Fake {
    now: Rc<Cell<Duration>>,
    cues: Vec<(Duration, Cue)>,
    sender: Option<Sender<Event>>,
    paints: Vec<(Duration, Frame)>,
    lowered: Vec<Duration>,
}

impl Fake {
    fn deliver(&mut self) {
        let now = self.now.get();
        let (due, later) = std::mem::take(&mut self.cues)
            .into_iter()
            .partition::<Vec<_>, _>(|(at, _)| *at <= now);
        self.cues = later;
        for (_, cue) in due {
            match cue {
                Cue::Send(event) => self.send(event),
                Cue::Hang => self.sender = None,
            }
        }
    }

    fn send(&self, event: Event) {
        if let Some(sender) = &self.sender {
            sender.send(event).unwrap();
        }
    }
}

impl Surface for Fake {
    fn wait(&mut self, timeout: Duration) {
        assert!(timeout > Duration::ZERO, "a zero wait spins");
        self.now.set(self.now.get() + timeout);
        self.deliver();
    }

    fn paint(&mut self, frame: Frame) {
        self.paints.push((self.now.get(), frame));
    }

    fn place(&mut self, _: PhysicalRect) {}

    fn lower(&mut self) {
        self.lowered.push(self.now.get());
    }
}

struct Run {
    paints: Vec<(Duration, Frame)>,
    lowered: Vec<Duration>,
    closed_at: Duration,
}

impl Run {
    fn first_paint(&self) -> Option<Duration> {
        self.paints.first().map(|(at, _)| *at)
    }
}

fn run(motion: Motion, cues: Vec<(Duration, Cue)>) -> Run {
    let (sender, events): (Sender<Event>, Receiver<Event>) = channel();
    let now = Rc::new(Cell::new(ms(5)));
    let mut fake = Fake {
        now: now.clone(),
        cues,
        sender: Some(sender),
        paints: Vec::new(),
        lowered: Vec::new(),
    };
    fake.deliver();
    let clock = now.clone();
    drive(motion, move || clock.get(), &events, &mut fake);
    Run {
        paints: fake.paints,
        lowered: fake.lowered,
        closed_at: now.get(),
    }
}

fn ready_at(at: u64) -> Vec<(Duration, Cue)> {
    vec![(ms(at), Cue::Send(Event::Ready))]
}

const LATE: Duration = Duration::from_millis(40);

#[test]
fn a_start_ready_at_150_ms_never_shows_the_splash() {
    let run = run(Motion::Animated, ready_at(150));
    assert!(run.paints.is_empty());
    assert!(run.closed_at <= ms(200) + LATE);
}

#[test]
fn a_start_not_ready_by_200_ms_shows_the_splash_at_200_ms() {
    let run = run(Motion::Animated, ready_at(1500));
    let shown = run.first_paint().unwrap();
    assert!(shown >= ms(200) && shown < ms(200) + LATE, "{shown:?}");
}

#[test]
fn a_shown_splash_stays_at_least_700_ms() {
    let run = run(Motion::Animated, ready_at(260));
    let shown = run.first_paint().unwrap();
    assert!(run.closed_at >= shown + ms(700), "{:?}", run.closed_at);
    assert!(
        run.closed_at < shown + ms(700) + LATE,
        "{:?}",
        run.closed_at
    );
}

#[test]
fn ready_after_the_minimum_closes_at_once() {
    let run = run(Motion::Animated, ready_at(2000));
    assert!(run.closed_at >= ms(2000) && run.closed_at < ms(2000) + LATE);
}

#[test]
fn the_slow_line_appears_at_3_s() {
    let run = run(Motion::Animated, ready_at(4000));
    let slow = run.paints.iter().find(|(_, frame)| frame.is_slow).unwrap();
    assert!(
        slow.0 >= ms(3000) && slow.0 < ms(3000) + LATE,
        "{:?}",
        slow.0
    );
    assert!(
        run.paints
            .iter()
            .all(|(at, frame)| frame.is_slow == (*at >= ms(3000)))
    );
}

#[test]
fn reduced_motion_keeps_the_bar_still() {
    let run = run(Motion::Still, ready_at(2500));
    assert!(!run.paints.is_empty());
    assert!(run.paints.iter().all(|(_, frame)| frame.bar == 1.0));
}

#[test]
fn the_bar_breathes_between_full_and_dim() {
    let run = run(Motion::Animated, ready_at(4000));
    let bars = run.paints.iter().map(|(_, frame)| frame.bar);
    let dimmest = bars.clone().fold(1.0_f32, f32::min);
    assert!((0.3..0.35).contains(&dimmest), "{dimmest}");
    assert!(bars.clone().all(|bar| (0.3..=1.0).contains(&bar)));
    assert!(bars.skip(1).any(|bar| bar > 0.95));
}

#[test]
fn the_main_thread_ending_early_closes_the_splash_at_once() {
    let run = run(Motion::Animated, vec![(ms(500), Cue::Hang)]);
    assert!(run.first_paint().is_some());
    assert!(run.closed_at < ms(500) + LATE);
}

#[test]
fn the_main_thread_ending_before_200_ms_never_shows_the_splash() {
    let run = run(Motion::Animated, vec![(ms(120), Cue::Hang)]);
    assert!(run.paints.is_empty());
}

#[test]
fn ready_then_hang_up_still_keeps_the_minimum() {
    let cues = vec![(ms(260), Cue::Send(Event::Ready)), (ms(270), Cue::Hang)];
    let run = run(Motion::Animated, cues);
    let shown = run.first_paint().unwrap();
    assert!(run.closed_at >= shown + ms(700));
}

#[test]
fn ready_while_shown_lowers_the_splash_below_the_main_window() {
    let run = run(Motion::Animated, ready_at(260));
    assert_eq!(run.lowered.len(), 1);
    assert!(run.lowered[0] >= ms(260) && run.lowered[0] < ms(260) + LATE);
    assert!(run.closed_at > run.lowered[0]);
}

#[test]
fn ready_before_the_splash_appears_lowers_nothing() {
    let run = run(Motion::Animated, ready_at(150));
    assert!(run.lowered.is_empty());
}

#[test]
fn a_splash_without_ready_closes_20_s_after_it_appeared() {
    for motion in [Motion::Animated, Motion::Still] {
        let run = run(motion, Vec::new());
        let shown = run.first_paint().unwrap();
        assert!(run.closed_at >= shown + ms(20_000), "{:?}", run.closed_at);
        assert!(
            run.closed_at < shown + ms(20_000) + LATE,
            "{:?}",
            run.closed_at
        );
    }
}
