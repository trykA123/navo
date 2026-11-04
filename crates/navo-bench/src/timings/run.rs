use std::cell::RefCell;

use super::error::TimingError;
use super::headless::headless;
use super::host::{Host, Subject};
use super::result::{GeneratedReport, HeadlessReport, KeysReport, TimingsReport, UsableReport};
use super::window::{first_usable, key_to_frame};
use crate::harness::phase::{Observer, Phase, PhaseResult, drive};
use crate::index::micros;
use crate::measure::Metric;
use crate::privacy::{Report, SafeText};
use crate::safe_text;

/// D98: the generated folder holds this many empty files.
pub const GENERATED_FILES: u32 = 100_000;

/// Which folders to time: the owner's, and the generated one unless `--quick`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Plan {
    pub owner: bool,
    pub generated: bool,
}

/// Hands out run numbers; each run gets its own profile and output files.
#[derive(Default)]
pub(super) struct Ids(u32);

impl Ids {
    pub(super) fn next(&mut self) -> u32 {
        self.0 += 1;
        self.0
    }
}

struct State<'a> {
    host: &'a mut dyn Host,
    ids: Ids,
    prepared: bool,
    built: bool,
    reports: Vec<Report>,
}

fn verdict(failure: Option<TimingError>) -> PhaseResult {
    failure.map_or(PhaseResult::Done, |error| PhaseResult::Failed(error.code()))
}

impl State<'_> {
    fn push(&mut self, report: TimingsReport) {
        self.reports.push(Report::Timings(report));
    }

    /// The reason a subject cannot be timed, if it cannot.
    fn blocked(&self, subject: Subject) -> Option<TimingError> {
        match subject {
            _ if !self.prepared => Some(TimingError::NotPrepared),
            Subject::Generated if !self.built => Some(TimingError::NotBuilt),
            _ => None,
        }
    }

    fn prepare(&mut self) -> PhaseResult {
        let result = self.host.prepare();
        self.prepared = result.is_ok();
        verdict(result.err())
    }

    fn generate(&mut self) -> PhaseResult {
        if !self.prepared {
            return PhaseResult::NotMeasured(TimingError::NotPrepared.reason());
        }
        let began = self.host.elapsed();
        let result = self.host.generate(GENERATED_FILES);
        let took = self.host.elapsed().saturating_sub(began);
        self.built = result.is_ok();
        let build_micros = match result {
            Ok(()) => Metric::Measured {
                value: micros(took.as_secs_f64() * 1000.0),
            },
            Err(error) => Metric::missing(error.reason()),
        };
        self.push(TimingsReport::Generated(GeneratedReport {
            files: GENERATED_FILES,
            build_micros,
        }));
        verdict(result.err())
    }

    fn headless(&mut self, subject: Subject) -> PhaseResult {
        if let Some(error) = self.blocked(subject) {
            self.push(TimingsReport::Headless(HeadlessReport::unavailable(
                subject,
                error.reason(),
            )));
            return PhaseResult::NotMeasured(error.reason());
        }
        let (report, failure) = headless(self.host, &mut self.ids, subject);
        self.push(TimingsReport::Headless(report));
        verdict(failure)
    }

    fn usable(&mut self, subject: Subject) -> PhaseResult {
        if let Some(error) = self.blocked(subject) {
            let report = UsableReport::unavailable(subject, error.reason());
            self.push(TimingsReport::FirstUsable(report));
            return PhaseResult::NotMeasured(error.reason());
        }
        let (report, failure) = first_usable(self.host, &mut self.ids, subject);
        self.push(TimingsReport::FirstUsable(report));
        verdict(failure)
    }

    fn keys(&mut self, subject: Subject) -> PhaseResult {
        if let Some(error) = self.blocked(subject) {
            let report = KeysReport::unavailable(subject, error.reason());
            self.push(TimingsReport::KeyToFrame(report));
            return PhaseResult::NotMeasured(error.reason());
        }
        let (report, failure) = key_to_frame(self.host, &mut self.ids, subject);
        self.push(TimingsReport::KeyToFrame(report));
        verdict(failure)
    }
}

fn names(subject: Subject) -> [SafeText; 3] {
    match subject {
        Subject::Owner => [
            safe_text!("owner folder: list, sort, filter"),
            safe_text!("owner folder: first usable"),
            safe_text!("owner folder: key to frame"),
        ],
        Subject::Generated => [
            safe_text!("100k folder: list, sort, filter"),
            safe_text!("100k folder: first usable"),
            safe_text!("100k folder: key to frame"),
        ],
    }
}

fn subject_phases<'a>(state: &'a RefCell<State<'_>>, subject: Subject) -> Vec<Phase<'a>> {
    let [list, usable, keys] = names(subject);
    vec![
        Phase::new(list, move || state.borrow_mut().headless(subject)),
        Phase::new(usable, move || state.borrow_mut().usable(subject)),
        Phase::new(keys, move || state.borrow_mut().keys(subject)),
    ]
}

/// Times the owner's folder and the generated 100k folder: build, headless list, sort and
/// filter, first usable, key-to-frame. A failed phase is recorded and never stops the rest.
pub fn measure_folder_timings(
    host: &mut dyn Host,
    plan: Plan,
    observer: &mut impl Observer,
) -> Vec<Report> {
    let state = RefCell::new(State {
        host,
        ids: Ids::default(),
        prepared: false,
        built: false,
        reports: Vec::new(),
    });
    let mut phases = vec![Phase::new(safe_text!("folder timings: prepare"), || {
        state.borrow_mut().prepare()
    })];
    if plan.generated {
        phases.push(Phase::new(safe_text!("100k folder: build"), || {
            state.borrow_mut().generate()
        }));
    }
    if plan.owner {
        phases.extend(subject_phases(&state, Subject::Owner));
    }
    if plan.generated {
        phases.extend(subject_phases(&state, Subject::Generated));
    }
    drive(phases, observer);
    state.into_inner().reports
}
