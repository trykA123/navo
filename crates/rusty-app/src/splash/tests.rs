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
    fades: Vec<(Duration, u8)>,
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

    fn fade(&mut self, alpha: u8) {
        self.fades.push((self.now.get(), alpha));
    }

    fn place(&mut self, _: PhysicalRect) {}

    fn lower(&mut self) {
        self.lowered.push(self.now.get());
    }
}

struct Run {
    paints: Vec<(Duration, Frame)>,
    fades: Vec<(Duration, u8)>,
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
        fades: Vec::new(),
        lowered: Vec::new(),
    };
    fake.deliver();
    let clock = now.clone();
    drive(motion, move || clock.get(), &events, &mut fake);
    Run {
        paints: fake.paints,
        fades: fake.fades,
        lowered: fake.lowered,
        closed_at: now.get(),
    }
}

fn ready_at(at: u64) -> Vec<(Duration, Cue)> {
    vec![(ms(at), Cue::Send(Event::Ready))]
}

const LATE: Duration = Duration::from_millis(40);
const FADE: Duration = Duration::from_millis(150);

#[test]
fn a_start_ready_at_450_ms_never_shows_the_splash() {
    let run = run(Motion::Animated, ready_at(450));
    assert!(run.paints.is_empty());
    assert!(run.closed_at <= ms(500) + LATE);
}

#[test]
fn a_start_not_ready_by_500_ms_shows_the_splash_at_500_ms() {
    let run = run(Motion::Animated, ready_at(1500));
    let shown = run.first_paint().unwrap();
    assert!(shown >= ms(500) && shown < ms(500) + LATE, "{shown:?}");
}

#[test]
fn a_shown_splash_stays_at_least_700_ms() {
    let run = run(Motion::Animated, ready_at(560));
    let shown = run.first_paint().unwrap();
    assert!(
        run.closed_at >= shown + ms(700) + FADE,
        "{:?}",
        run.closed_at
    );
    assert!(
        run.closed_at < shown + ms(700) + FADE + LATE,
        "{:?}",
        run.closed_at
    );
}

#[test]
fn ready_after_the_minimum_fades_out_and_closes() {
    let run = run(Motion::Animated, ready_at(2000));
    assert!(run.closed_at >= ms(2000) + FADE && run.closed_at < ms(2000) + FADE + LATE);
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
fn the_main_thread_ending_early_fades_the_splash_out() {
    let run = run(Motion::Animated, vec![(ms(800), Cue::Hang)]);
    assert!(run.first_paint().is_some());
    assert!(run.closed_at < ms(800) + FADE + LATE);
}

#[test]
fn the_main_thread_ending_before_500_ms_never_shows_the_splash() {
    let run = run(Motion::Animated, vec![(ms(120), Cue::Hang)]);
    assert!(run.paints.is_empty());
}

#[test]
fn ready_then_hang_up_still_keeps_the_minimum() {
    let cues = vec![(ms(560), Cue::Send(Event::Ready)), (ms(570), Cue::Hang)];
    let run = run(Motion::Animated, cues);
    let shown = run.first_paint().unwrap();
    assert!(run.closed_at >= shown + ms(700) + FADE);
}

#[test]
fn ready_while_shown_lowers_the_splash_below_the_main_window() {
    let run = run(Motion::Animated, ready_at(560));
    assert_eq!(run.lowered.len(), 1);
    assert!(run.lowered[0] >= ms(560) && run.lowered[0] < ms(560) + LATE);
    assert!(run.closed_at > run.lowered[0]);
}

#[test]
fn ready_before_the_splash_appears_lowers_nothing() {
    let run = run(Motion::Animated, ready_at(450));
    assert!(run.lowered.is_empty());
}

#[test]
fn a_splash_without_ready_closes_20_s_after_it_appeared() {
    for (motion, fade) in [(Motion::Animated, FADE), (Motion::Still, Duration::ZERO)] {
        let run = run(motion, Vec::new());
        let shown = run.first_paint().unwrap();
        assert!(
            run.closed_at >= shown + ms(20_000) + fade,
            "{:?}",
            run.closed_at
        );
        assert!(
            run.closed_at < shown + ms(20_000) + fade + LATE,
            "{:?}",
            run.closed_at
        );
    }
}

#[test]
fn closing_a_shown_splash_fades_alpha_down_within_150_ms() {
    let run = run(Motion::Animated, ready_at(2000));
    let alphas: Vec<u8> = run.fades.iter().map(|(_, alpha)| *alpha).collect();
    assert_eq!(alphas.first(), Some(&255));
    assert!(alphas.len() > 2);
    assert!(
        alphas.windows(2).all(|pair| pair[1] < pair[0]),
        "{alphas:?}"
    );
    assert!(run.fades.iter().all(|(at, _)| *at < ms(2000) + FADE + LATE));
    assert!(run.closed_at <= ms(2000) + FADE + LATE);
}

#[test]
fn reduced_motion_closes_at_once_without_a_fade() {
    let run = run(Motion::Still, ready_at(2000));
    assert!(run.fades.is_empty());
    assert!(run.closed_at >= ms(2000) && run.closed_at < ms(2000) + LATE);
}

#[test]
fn a_splash_that_never_showed_does_not_fade() {
    let run = run(Motion::Animated, ready_at(450));
    assert!(run.fades.is_empty());
}
