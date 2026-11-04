use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use super::error::TimingError;
use super::host::{Capture, Host, Launch, Subject, Window};
use crate::probe::{CloseOutcome, Key, ProbeError};

pub(crate) const SECRET: &str = "zqxjv-secret-label";

/// Bench JSON with five runs; strings in it are planted to prove they are dropped.
pub(super) fn bench_text(run: u32) -> String {
    let base = f64::from(run);
    let one = |at: f64| {
        format!(
            r#"{{"list_ms":{},"sort_ms":[["size",{}],["date",{}],["extension",{}],["name",{}],["{SECRET}",9.0]],
            "filter_keystroke_ms":[{},{},{}]}}"#,
            10.0 + base + at,
            1.0 + at,
            2.0 + at,
            3.0 + at,
            4.0 + at,
            0.5 + at,
            0.6 + at,
            0.7 + at
        )
    };
    let runs: Vec<String> = (0..5).map(|at| one(f64::from(at))).collect();
    format!(
        r#"{{"label":"{SECRET}","entries":100000,"bytes":4096,"filter_chars":3,"runs":[{}]}}"#,
        runs.join(",")
    )
}

pub(super) fn metrics_text(run: u32) -> String {
    format!(
        r#"{{"renderer":"{SECRET}","first_frame_ms":{}.0,"first_usable_ms":{}.0,"marks":[]}}"#,
        100 + run,
        200 + run
    )
}

/// Trace lines of `count` records for a run; the action text is planted.
pub(super) fn trace_text(run: u32, count: u32) -> String {
    let mut text: String = (0..count)
        .map(|at| {
            format!(
                "{{\"action\":\"key:{SECRET}\",\"key_to_frame_ms\":{}.5,\"ui_busy_ms\":1.0,\"disk_calls\":0}}\n",
                5 + at + run % 3
            )
        })
        .collect();
    text.push_str("{\"action\":\"key:s\",\"key_to");
    text
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Bench {
    Good,
    ExitsNonZero,
    Garbage,
}

pub(crate) struct Fake {
    pub(crate) events: Rc<RefCell<Vec<String>>>,
    pub(super) clock: Duration,
    pub(crate) fail_prepare: bool,
    pub(super) fail_generate: bool,
    pub(super) bench: Bench,
    pub(super) no_metrics: bool,
    pub(super) trace_records: Option<u32>,
    pub(super) fail_launch: Vec<u32>,
    reads: Cell<u32>,
}

impl Fake {
    pub(crate) fn new() -> Self {
        Self {
            events: Rc::default(),
            clock: Duration::ZERO,
            fail_prepare: false,
            fail_generate: false,
            bench: Bench::Good,
            no_metrics: false,
            trace_records: Some(4),
            fail_launch: Vec::new(),
            reads: Cell::new(0),
        }
    }

    fn log(&self, event: String) {
        self.events.borrow_mut().push(event);
    }

    pub(super) fn events(&self) -> Vec<String> {
        self.events.borrow().clone()
    }
}

fn subject(subject: Subject) -> &'static str {
    match subject {
        Subject::Owner => "owner",
        Subject::Generated => "generated",
    }
}

impl Host for Fake {
    fn prepare(&mut self) -> Result<(), TimingError> {
        self.log("prepare".into());
        if self.fail_prepare {
            return Err(TimingError::Io(std::io::ErrorKind::PermissionDenied));
        }
        Ok(())
    }

    fn generate(&mut self, files: u32) -> Result<(), TimingError> {
        self.log(format!("generate {files}"));
        self.clock += Duration::from_millis(2_500);
        if self.fail_generate {
            return Err(TimingError::Io(std::io::ErrorKind::OutOfMemory));
        }
        Ok(())
    }

    fn run_bench(&mut self, which: Subject, run: u32) -> Result<String, TimingError> {
        self.log(format!("bench {} {run}", subject(which)));
        match self.bench {
            Bench::Good => Ok(bench_text(run)),
            Bench::ExitsNonZero => Err(TimingError::BenchExit),
            Bench::Garbage => Ok(format!("{{\"entries\":\"{SECRET}\"")),
        }
    }

    fn launch(&mut self, launch: Launch) -> Result<Box<dyn Window>, TimingError> {
        self.log(format!(
            "launch {} {} metrics={} trace={}",
            subject(launch.subject),
            launch.run,
            launch.metrics,
            launch.trace
        ));
        if self.fail_launch.contains(&launch.run) {
            return Err(ProbeError::Exited.into());
        }
        self.reads.set(0);
        Ok(Box::new(FakeWindow {
            run: launch.run,
            events: Rc::clone(&self.events),
        }))
    }

    fn read(&mut self, capture: Capture, run: u32) -> Option<String> {
        match capture {
            Capture::Metrics => {
                self.reads.set(self.reads.get() + 1);
                (!self.no_metrics).then(|| metrics_text(run))
            }
            Capture::Trace => self.trace_records.map(|count| trace_text(run, count)),
        }
    }

    fn pause(&mut self, time: Duration) {
        self.clock += time;
    }

    fn elapsed(&self) -> Duration {
        self.clock
    }
}

struct FakeWindow {
    run: u32,
    events: Rc<RefCell<Vec<String>>>,
}

impl Window for FakeWindow {
    fn wait_for_window(&mut self, _timeout: Duration) -> Result<Duration, TimingError> {
        Ok(Duration::from_millis(100))
    }

    fn post_keys(&mut self, keys: &[Key]) -> Result<(), TimingError> {
        self.events
            .borrow_mut()
            .push(format!("post {} {keys:?}", self.run));
        Ok(())
    }

    fn close(self: Box<Self>, _deadline: Duration) -> Result<CloseOutcome, TimingError> {
        self.events.borrow_mut().push(format!("close {}", self.run));
        Ok(CloseOutcome {
            polite: true,
            exit_code: Some(0),
        })
    }
}
