use std::time::{Duration, Instant};

use rusty_diag::perf::{FrameSample, Latency, Recording, UiFlag, UiState, ms};

use super::FrameStart;

const WINDOWS: usize = 8;
const SCROLL_LINGER: Duration = Duration::from_millis(150);
const ARMED_FOR: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    Idle,
    Armed(Instant),
    Ready(Instant),
}

#[derive(Clone, Copy, Debug)]
struct Running {
    start: Instant,
    window: Option<u64>,
    frame: u64,
}

#[derive(Clone, Copy, Debug)]
struct WindowClock {
    id: u64,
    last_start: Instant,
    was_moving: bool,
}

pub(super) struct Recorder {
    pub(super) recording: Recording,
    windows: [Option<WindowClock>; WINDOWS],
    pending: [Pending; Latency::ALL.len()],
    animations: [Option<Running>; Latency::ALL.len()],
    frame_flags: UiState,
    frame: u64,
    frame_window: Option<u64>,
    last_scroll: Option<Instant>,
}

impl Recorder {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            recording: Recording::new(now),
            windows: [None; WINDOWS],
            pending: [Pending::Idle; Latency::ALL.len()],
            animations: [None; Latency::ALL.len()],
            frame_flags: UiState::default(),
            frame: 0,
            frame_window: None,
            last_scroll: None,
        }
    }

    pub(super) fn into_recording(self) -> Recording {
        self.recording
    }

    pub(super) fn begin(&mut self, kind: Latency, now: Instant) {
        let slot = &mut self.pending[kind.index()];
        match *slot {
            Pending::Idle => *slot = Pending::Armed(now),
            Pending::Armed(start) if is_stale(start, now) => *slot = Pending::Armed(now),
            Pending::Armed(_) | Pending::Ready(_) => {}
        }
    }

    pub(super) fn ready(&mut self, kind: Latency, now: Instant) {
        let slot = &mut self.pending[kind.index()];
        if let Pending::Armed(start) = *slot {
            *slot = if is_stale(start, now) {
                Pending::Idle
            } else {
                Pending::Ready(start)
            };
        }
    }

    pub(super) fn cancel(&mut self, kind: Latency) {
        let slot = &mut self.pending[kind.index()];
        if matches!(*slot, Pending::Armed(_)) {
            *slot = Pending::Idle;
        }
    }

    pub(super) fn animating(&mut self, kind: Latency, is_running: bool, now: Instant) {
        let slot = &mut self.animations[kind.index()];
        if is_running {
            let start = slot.map_or(now, |running| running.start);
            *slot = Some(Running {
                start,
                window: self.frame_window,
                frame: self.frame,
            });
            self.frame_flags = self.frame_flags.with(UiFlag::Animation);
        } else if let Some(running) = slot.take() {
            self.recording.push_latency(kind, running.start, now);
        }
    }

    pub(super) fn note(&mut self, flag: UiFlag) {
        self.frame_flags = self.frame_flags.with(flag);
    }

    pub(super) fn note_scroll(&mut self, now: Instant) {
        self.last_scroll = Some(now);
        self.begin(Latency::ScrollPaint, now);
        self.ready(Latency::ScrollPaint, now);
    }

    pub(super) fn clear_frame(&mut self, window: u64) {
        self.frame_flags = UiState::default();
        self.frame = self.frame.wrapping_add(1);
        self.frame_window = Some(window);
    }

    pub(super) fn finish_frame(&mut self, start: FrameStart, laid_out: Instant, painted: Instant) {
        let is_scrolling = self
            .last_scroll
            .is_some_and(|at| start.at.saturating_duration_since(at) < SCROLL_LINGER);
        let state = start
            .state
            .union(self.frame_flags)
            .when(is_scrolling, UiFlag::Scroll);
        let (window, interval) = self.clock(start.window, start.at, state);
        let sample = FrameSample {
            window,
            at_ms: self.recording.at_ms(start.at),
            interval_ms: interval.map(ms),
            layout_ms: ms(laid_out.saturating_duration_since(start.at)),
            paint_ms: ms(painted.saturating_duration_since(laid_out)),
            state,
        };
        self.recording.frames.push(sample);
        self.frame_flags = UiState::default();
        self.drop_unrefreshed(start.window);
        self.settle_pending(painted);
    }

    fn drop_unrefreshed(&mut self, window: u64) {
        let frame = self.frame;
        for slot in &mut self.animations {
            if slot.is_some_and(|running| running.window == Some(window) && running.frame != frame)
            {
                *slot = None;
            }
        }
    }

    fn clock(&mut self, id: u64, at: Instant, state: UiState) -> (u8, Option<Duration>) {
        let index = self
            .windows
            .iter()
            .position(|clock| clock.is_some_and(|clock| clock.id == id))
            .or_else(|| self.windows.iter().position(Option::is_none))
            .unwrap_or(WINDOWS - 1);
        let previous = self.windows[index].filter(|clock| clock.id == id);
        let interval = previous
            .filter(|clock| clock.was_moving)
            .map(|clock| at.saturating_duration_since(clock.last_start));
        self.windows[index] = Some(WindowClock {
            id,
            last_start: at,
            was_moving: state.has(UiFlag::Animation) || state.has(UiFlag::Scroll),
        });
        (u8::try_from(index).unwrap_or(u8::MAX), interval)
    }

    fn settle_pending(&mut self, painted: Instant) {
        for kind in Latency::ALL {
            match self.pending[kind.index()] {
                Pending::Ready(start) => {
                    self.recording.push_latency(kind, start, painted);
                    self.pending[kind.index()] = Pending::Idle;
                }
                Pending::Armed(start) if is_stale(start, painted) => {
                    self.pending[kind.index()] = Pending::Idle;
                }
                Pending::Armed(_) | Pending::Idle => {}
            }
        }
    }
}

fn is_stale(start: Instant, now: Instant) -> bool {
    now.saturating_duration_since(start) > ARMED_FOR
}
