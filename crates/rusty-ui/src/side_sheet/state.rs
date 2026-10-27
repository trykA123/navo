use std::time::Instant;

use gpui_kit::{App, Context, Pixels, px};

use super::frame::SheetFrame;
use super::motion;
use crate::tokens;

const STEP: f32 = 48.;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Narrower,
    Wider,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Phase {
    Closed,
    Open,
    Leaving(Instant),
}

#[derive(Debug)]
pub struct SheetState {
    delta: Pixels,
    phase: Phase,
    opened: usize,
}

impl Default for SheetState {
    fn default() -> Self {
        Self::new()
    }
}

impl SheetState {
    pub fn new() -> Self {
        Self {
            delta: px(0.),
            phase: Phase::Closed,
            opened: 0,
        }
    }

    pub fn is_open(&self) -> bool {
        self.phase == Phase::Open
    }

    pub fn is_shown(&self, cx: &App) -> bool {
        match self.phase {
            Phase::Closed => false,
            Phase::Open => true,
            Phase::Leaving(since) => {
                let now = cx.background_executor().now();
                !motion::has_left(since, now, tokens::current(cx).motion.fade)
            }
        }
    }

    pub fn delta(&self) -> Pixels {
        self.delta
    }

    pub fn width(&self, frame: &SheetFrame) -> Pixels {
        frame.width(self.delta)
    }

    pub fn open(&mut self, cx: &mut Context<Self>) {
        if !self.is_open() {
            self.opened += 1;
            self.phase = Phase::Open;
            cx.notify();
        }
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.is_open() {
            self.phase = Phase::Leaving(cx.background_executor().now());
            cx.notify();
        }
    }

    pub fn step(&mut self, step: Step, frame: &SheetFrame) {
        let by = match step {
            Step::Narrower => -STEP,
            Step::Wider => STEP,
        };
        self.resize_to(self.width(frame) + px(by), frame);
    }

    pub fn resize_to(&mut self, width: Pixels, frame: &SheetFrame) {
        self.delta = frame.delta_for(width);
    }

    pub(super) fn phase(&self) -> Phase {
        self.phase
    }

    pub(super) fn opened(&self) -> usize {
        self.opened
    }
}
