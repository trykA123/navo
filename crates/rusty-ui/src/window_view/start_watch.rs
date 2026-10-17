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
        self.marks.push((label, Instant::now()));
    }
}

#[derive(Default)]
pub(super) struct StartClock {
    first_frame: Option<Instant>,
    first_usable: Option<Instant>,
}

impl StartClock {
    pub(super) fn frame(&mut self, at: Instant, is_listed: bool) -> bool {
        self.first_frame.get_or_insert(at);
        if self.first_usable.is_some() || !is_listed {
            return false;
        }
        self.first_usable = Some(at);
        true
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
    renderer: Option<String>,
    is_frame_requested: bool,
}

struct Frame {
    at: Instant,
    renderer: Option<String>,
    is_listed: bool,
}

type Reported = (StartReport, Box<dyn FnOnce(StartReport)>);

impl Watching {
    pub(super) fn built(mut watch: StartWatch) -> Self {
        watch.mark("window built");
        Self {
            watch,
            clock: StartClock::default(),
            renderer: None,
            is_frame_requested: false,
        }
    }

    fn into_report(self) -> Option<Reported> {
        let report = StartReport {
            first_frame: self.clock.first_frame()?,
            first_usable: self.clock.first_usable()?,
            renderer: self.renderer,
            marks: self.watch.marks,
        };
        Some((report, self.watch.on_report))
    }
}

impl FerriteWindow {
    pub(super) fn watch_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let is_listed = self.panes_listed();
        let Some(start) = self.start.as_mut() else {
            return;
        };
        if start.is_frame_requested || start.clock.first_usable().is_some() {
            return;
        }
        start.is_frame_requested = true;
        let this = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            let at = Instant::now();
            let renderer = window.gpu_specs().map(|specs| specs.device_name);
            let frame = Frame {
                at,
                renderer,
                is_listed,
            };
            this.update(cx, |view, cx| view.start_frame_drawn(frame, cx))
                .ok();
        });
    }

    fn start_frame_drawn(&mut self, frame: Frame, cx: &mut Context<Self>) {
        let is_listed_now = self.panes_listed();
        let Some(start) = self.start.as_mut() else {
            return;
        };
        start.is_frame_requested = false;
        start.renderer = start.renderer.take().or(frame.renderer);
        if !start.clock.frame(frame.at, frame.is_listed) {
            if is_listed_now {
                cx.notify();
            }
            return;
        }
        let idle = start.watch.idle;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(idle).await;
            this.update(cx, |view, cx| view.report_start(cx)).ok();
        })
        .detach();
    }

    fn report_start(&mut self, cx: &mut Context<Self>) {
        let Some((report, on_report)) = self.start.take().and_then(Watching::into_report) else {
            return;
        };
        on_report(report);
        cx.quit();
    }

    fn panes_listed(&self) -> bool {
        let state = self.browser.state();
        let panes = self.panes();
        !panes.is_empty()
            && panes.iter().all(|pane| {
                state
                    .pane(*pane)
                    .is_some_and(|pane| pane.active().listing().is_some())
            })
    }

    #[cfg(test)]
    pub(super) fn is_start_watched(&self) -> bool {
        self.start.is_some()
    }
}
