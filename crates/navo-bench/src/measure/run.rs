use std::cell::RefCell;

use super::host::{Host, MeasureError};
use super::idle::idle;
use super::result::{IdleReport, MeasureReport, Metric, StartKind, StartReport};
use super::start::{Ids, Outcome, cold, warm};
use crate::harness::phase::{Observer, Phase, PhaseResult, drive};
use crate::privacy::{Report, SafeText};
use crate::safe_text;

struct State<'a> {
    host: &'a mut dyn Host,
    ids: Ids,
    prepared: bool,
    reports: Vec<Report>,
}

impl State<'_> {
    fn push(&mut self, report: MeasureReport) {
        self.reports.push(Report::Measure(report));
    }

    fn prepare(&mut self) -> PhaseResult {
        match self.host.prepare() {
            Ok(()) => {
                self.prepared = true;
                PhaseResult::Done
            }
            Err(error) => PhaseResult::Failed(error.code()),
        }
    }

    fn start(&mut self, kind: StartKind) -> PhaseResult {
        if !self.prepared {
            let reason = not_prepared();
            self.push(MeasureReport::Start(StartReport::unavailable(kind, reason)));
            return PhaseResult::NotMeasured(reason);
        }
        let Outcome { report, failure } = match kind {
            StartKind::Cold => cold(self.host, &mut self.ids),
            StartKind::Warm => warm(self.host, &mut self.ids),
        };
        self.push(MeasureReport::Start(report));
        verdict(failure)
    }

    fn idle(&mut self) -> PhaseResult {
        if !self.prepared {
            let reason = not_prepared();
            self.push(MeasureReport::Idle(IdleReport::unavailable(reason)));
            return PhaseResult::NotMeasured(reason);
        }
        let run = self.ids.next();
        let (report, failure) = idle(self.host, run);
        self.push(MeasureReport::Idle(report));
        verdict(failure)
    }

    fn exe_size(&mut self) -> PhaseResult {
        let size = self.host.exe_size();
        let failure = size.as_ref().err().copied();
        let bytes = Metric::from_result(size);
        self.push(MeasureReport::ExeSize { bytes });
        verdict(failure)
    }
}

fn not_prepared() -> SafeText {
    safe_text!("fixtures not built")
}

fn verdict(failure: Option<MeasureError>) -> PhaseResult {
    failure.map_or(PhaseResult::Done, |error| PhaseResult::Failed(error.code()))
}

/// Runs the start and idle measures in order: fixtures, cold, warm, idle, exe size.
/// A failed run or phase is recorded and never stops the rest.
pub fn measure_start_and_idle(host: &mut dyn Host, observer: &mut impl Observer) -> Vec<Report> {
    let state = RefCell::new(State {
        host,
        ids: Ids::default(),
        prepared: false,
        reports: Vec::new(),
    });
    let phases = vec![
        Phase::new(safe_text!("prepare fixtures"), || {
            state.borrow_mut().prepare()
        }),
        Phase::new(safe_text!("cold start"), || {
            state.borrow_mut().start(StartKind::Cold)
        }),
        Phase::new(safe_text!("warm start"), || {
            state.borrow_mut().start(StartKind::Warm)
        }),
        Phase::new(safe_text!("idle"), || state.borrow_mut().idle()),
        Phase::new(safe_text!("exe size"), || state.borrow_mut().exe_size()),
    ];
    drive(phases, observer);
    state.into_inner().reports
}
