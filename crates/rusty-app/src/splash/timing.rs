#![cfg_attr(
    not(any(windows, test)),
    expect(dead_code, reason = "only the Windows splash drives the timing")
)]

use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use rusty_core::placement::PhysicalRect;

const SHOW_AFTER: Duration = Duration::from_millis(200);
const MIN_SHOWN: Duration = Duration::from_millis(700);
const MAX_SHOWN: Duration = Duration::from_secs(20);
const SLOW_AFTER: Duration = Duration::from_secs(3);
const FADE: Duration = Duration::from_millis(1800);
const FRAME: Duration = Duration::from_millis(33);
const STILL: Duration = Duration::from_millis(100);
const DIMMEST: f32 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Event {
    Ready,
    Near(PhysicalRect),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Motion {
    Animated,
    Still,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Frame {
    pub(super) bar: f32,
    pub(super) is_slow: bool,
}

pub(super) trait Surface {
    fn wait(&mut self, timeout: Duration);
    fn paint(&mut self, frame: Frame);
    fn place(&mut self, near: PhysicalRect);
    fn lower(&mut self);
}

enum Step {
    Hidden(Duration),
    Visible(Frame, Duration),
    Closed,
}

enum Signal {
    Ready,
    HungUp,
}

struct Timing {
    motion: Motion,
    shown_at: Option<Duration>,
    close_at: Option<Duration>,
}

impl Timing {
    fn on(&mut self, signal: Signal, now: Duration) {
        let close_at = match signal {
            Signal::Ready => self
                .shown_at
                .map_or(now, |shown| (shown + MIN_SHOWN).max(now)),
            Signal::HungUp if self.close_at.is_some() => return,
            Signal::HungUp => now,
        };
        self.close_at = Some(self.close_at.map_or(close_at, |at| at.min(close_at)));
    }

    fn step(&mut self, now: Duration) -> Step {
        if self.close_at.is_some_and(|at| at <= now) {
            return Step::Closed;
        }
        let Some(shown) = self.shown_at.or((now >= SHOW_AFTER).then_some(now)) else {
            return Step::Hidden(SHOW_AFTER - now);
        };
        self.shown_at = Some(shown);
        if now >= shown + MAX_SHOWN {
            return Step::Closed;
        }
        let frame = Frame {
            bar: self.bar(now - shown),
            is_slow: now >= SLOW_AFTER,
        };
        let tick = match self.motion {
            Motion::Animated => FRAME,
            Motion::Still => STILL,
        };
        let next = [
            Some(tick),
            self.close_at.map(|at| at - now),
            Some(shown + MAX_SHOWN - now),
            Some(SLOW_AFTER.saturating_sub(now)).filter(|wait| !wait.is_zero()),
        ]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(tick);
        Step::Visible(frame, next)
    }

    fn bar(&self, shown_for: Duration) -> f32 {
        if self.motion == Motion::Still {
            return 1.0;
        }
        let phase = (shown_for.as_secs_f32() / FADE.as_secs_f32()) % 2.0;
        let half = if phase > 1.0 { 2.0 - phase } else { phase };
        let eased = half * half * (3.0 - 2.0 * half);
        1.0 - (1.0 - DIMMEST) * eased
    }
}

pub(super) fn drive(
    motion: Motion,
    clock: impl Fn() -> Duration,
    events: &Receiver<Event>,
    surface: &mut impl Surface,
) {
    let mut timing = Timing {
        motion,
        shown_at: None,
        close_at: None,
    };
    loop {
        receive(events, &mut timing, &clock, surface);
        match timing.step(clock()) {
            Step::Hidden(wait) => surface.wait(wait),
            Step::Visible(frame, next) => {
                surface.paint(frame);
                surface.wait(next);
            }
            Step::Closed => return,
        }
    }
}

fn receive(
    events: &Receiver<Event>,
    timing: &mut Timing,
    clock: &impl Fn() -> Duration,
    surface: &mut impl Surface,
) {
    loop {
        match events.try_recv() {
            Ok(Event::Near(near)) => surface.place(near),
            Ok(Event::Ready) => {
                timing.on(Signal::Ready, clock());
                if timing.shown_at.is_some() {
                    surface.lower();
                }
            }
            Err(TryRecvError::Disconnected) => return timing.on(Signal::HungUp, clock()),
            Err(TryRecvError::Empty) => return,
        }
    }
}
