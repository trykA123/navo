use std::time::{Duration, Instant};

use super::FerriteWindow;
use crate::prelude::*;

pub type Mark = (&'static str, Instant);

pub struct StartReport {
    pub first_frame: Instant,
    pub first_usable: Instant,
    pub renderer: Option<String>,
    pub marks: Vec<Mark>,
}

pub struct StartWatch {
    idle: Duration,
    marks: Vec<Mark>,
    on_report: Box<dyn FnOnce(StartReport)>,
}

impl StartWatch {
    pub fn new(idle: Duration, on_report: impl FnOnce(StartReport) + 'static) -> Self {
        Self {
            idle,
            marks: Vec::new(),
            on_report: Box::new(on_report),
        }
    }

    pub(super) fn mark(&mut self, label: &'static str) {
        let _ = label;
    }
}

#[derive(Default)]
pub(super) struct StartClock {
    first_frame: Option<Instant>,
    first_usable: Option<Instant>,
}

impl StartClock {
    pub(super) fn frame(&mut self, at: Instant, is_listed: bool) -> bool {
        let _ = (at, is_listed);
        false
    }

    pub(super) fn first_frame(&self) -> Option<Instant> {
        self.first_frame
    }

    pub(super) fn first_usable(&self) -> Option<Instant> {
        self.first_usable
    }
}

pub(super) struct Watching {
    watch: StartWatch,
    clock: StartClock,
}

impl Watching {
    pub(super) fn new(watch: StartWatch) -> Self {
        Self {
            watch,
            clock: StartClock::default(),
        }
    }
}

impl FerriteWindow {
    pub(super) fn watch_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = (window, cx);
    }

    pub(super) fn mark_start(&mut self, label: &'static str) {
        let _ = label;
    }

    pub(super) fn is_start_watched(&self) -> bool {
        self.start.is_some()
    }
}
