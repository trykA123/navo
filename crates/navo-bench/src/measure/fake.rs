use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use super::host::{Exe, Host, MeasureError, Session, Spec};
use crate::probe::{CloseOutcome, CpuTime, Memory, ProbeError, ThreadSwitches};

pub(crate) const SECRET: &str = "zqxjv-secret-renderer";

/// Metrics text for a run: first frame `100 + run`, first usable `200 + run`.
pub(super) fn sample_metrics(run: u32) -> String {
    format!(
        r#"{{"version":"0.4.0","commit":"abc","exe_bytes":1,"renderer":"{SECRET}",
        "first_frame_ms":{}.0,"first_usable_ms":{}.0,"marks":[],"idle_private_bytes":1}}"#,
        100 + run,
        200 + run
    )
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Metrics {
    Present,
    Absent,
    /// Garbage on the first polls, valid afterwards.
    HalfWritten(u32),
    /// Valid JSON that holds no start numbers.
    Empty,
}

pub(crate) struct Fake {
    pub(crate) events: Rc<RefCell<Vec<String>>>,
    pub(super) clock: Duration,
    pub(crate) fail_prepare: bool,
    pub(super) fail_copy: Vec<u32>,
    pub(super) fail_launch: Vec<u32>,
    pub(super) no_exe: bool,
    pub(super) metrics: Metrics,
    pub(super) dead_after_window: bool,
    reads: Cell<u32>,
}

impl Fake {
    pub(crate) fn new() -> Self {
        Self {
            events: Rc::default(),
            clock: Duration::ZERO,
            fail_prepare: false,
            fail_copy: Vec::new(),
            fail_launch: Vec::new(),
            no_exe: false,
            metrics: Metrics::Present,
            dead_after_window: false,
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

impl Host for Fake {
    fn prepare(&mut self) -> Result<(), MeasureError> {
        self.log("prepare".into());
        if self.fail_prepare {
            return Err(MeasureError::Io(std::io::ErrorKind::PermissionDenied));
        }
        Ok(())
    }

    fn copy_exe(&mut self, copy: u32) -> Result<(), MeasureError> {
        self.log(format!("copy {copy}"));
        if self.fail_copy.contains(&copy) {
            return Err(MeasureError::Io(std::io::ErrorKind::OutOfMemory));
        }
        Ok(())
    }

    fn launch(&mut self, spec: Spec) -> Result<Box<dyn Session>, MeasureError> {
        let exe = match spec.exe {
            Exe::Original => "original".to_owned(),
            Exe::Copy(copy) => format!("copy{copy}"),
        };
        self.log(format!(
            "launch {} {exe} metrics={}",
            spec.run, spec.metrics
        ));
        if self.fail_launch.contains(&spec.run) {
            return Err(ProbeError::Exited.into());
        }
        self.reads.set(0);
        Ok(Box::new(FakeSession {
            run: spec.run,
            events: Rc::clone(&self.events),
            dead: self.dead_after_window,
            memory_calls: Cell::new(0),
            cpu_calls: Cell::new(0),
            switch_calls: Cell::new(0),
        }))
    }

    fn read_metrics(&mut self, run: u32) -> Option<String> {
        let read = self.reads.get();
        self.reads.set(read + 1);
        match self.metrics {
            Metrics::Absent => None,
            Metrics::Empty => Some(r#"{"version":"1","marks":[]}"#.to_owned()),
            Metrics::HalfWritten(polls) if read < polls => Some(r#"{"first_fr"#.to_owned()),
            _ => Some(sample_metrics(run)),
        }
    }

    fn pause(&mut self, time: Duration) {
        self.clock += time;
        if time >= Duration::from_secs(1) {
            self.log(format!("pause {}", time.as_secs()));
        }
    }

    fn elapsed(&self) -> Duration {
        self.clock
    }

    fn exe_size(&self) -> Result<u64, MeasureError> {
        if self.no_exe {
            return Err(MeasureError::Io(std::io::ErrorKind::NotFound));
        }
        Ok(98_765_432)
    }
}

struct FakeSession {
    run: u32,
    events: Rc<RefCell<Vec<String>>>,
    dead: bool,
    memory_calls: Cell<u64>,
    cpu_calls: Cell<u64>,
    switch_calls: Cell<u64>,
}

fn bump(counter: &Cell<u64>) -> u64 {
    let now = counter.get();
    counter.set(now + 1);
    now
}

impl Session for FakeSession {
    fn wait_for_window(&mut self, _timeout: Duration) -> Result<Duration, MeasureError> {
        Ok(Duration::from_millis(100 + u64::from(self.run)))
    }

    fn memory(&self) -> Result<Memory, MeasureError> {
        if self.dead {
            return Err(ProbeError::Exited.into());
        }
        let call = bump(&self.memory_calls);
        Ok(Memory {
            private_bytes: 50_000_000 + call,
            working_set_bytes: 70_000_000 + call,
        })
    }

    fn cpu_time(&self) -> Result<CpuTime, MeasureError> {
        let call = bump(&self.cpu_calls);
        Ok(CpuTime {
            kernel_ticks: call * 1_000_000,
            user_ticks: call * 5_000_000,
        })
    }

    fn thread_switches(&self) -> Result<Vec<ThreadSwitches>, MeasureError> {
        let call = u32::try_from(bump(&self.switch_calls)).unwrap_or(0);
        let counters = [10_u32, 20, 30]
            .iter()
            .zip([300_u32, 60, 30])
            .enumerate()
            .map(|(thread, (base, per_call))| ThreadSwitches {
                thread_id: u32::try_from(thread).unwrap_or(0) + 1,
                switches: base + per_call * call,
            })
            .collect();
        Ok(counters)
    }

    fn close(self: Box<Self>, _deadline: Duration) -> Result<CloseOutcome, MeasureError> {
        self.events.borrow_mut().push(format!("close {}", self.run));
        Ok(CloseOutcome {
            polite: true,
            exit_code: Some(0),
        })
    }
}
