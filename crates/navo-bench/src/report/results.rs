use super::safety::RunSummary;
use super::timings::Timings;
use crate::index::IndexReport;
use crate::machine::MachineSummary;
use crate::measure::{IdleReport, MeasureReport, Metric, StartKind, StartReport};
use crate::privacy::Report;

/// Every typed result of a run, sorted by section.
#[derive(Clone, Debug)]
pub struct Results {
    pub cold: Option<StartReport>,
    pub warm: Option<StartReport>,
    pub idle: Option<IdleReport>,
    pub exe_size: Option<Metric<u64>>,
    pub index: Option<IndexReport>,
    pub machine: Option<MachineSummary>,
    /// Measurement, Bucket and Extension rows of the data profile.
    pub profile: Vec<Report>,
    pub timings: Timings,
    pub run: Option<RunSummary>,
}

impl Results {
    pub fn from_reports(reports: &[Report], timings: Timings) -> Self {
        let mut results = Self {
            cold: None,
            warm: None,
            idle: None,
            exe_size: None,
            index: None,
            machine: None,
            profile: Vec::new(),
            timings,
            run: None,
        };
        for report in reports {
            results.add(report);
        }
        results
    }

    fn add(&mut self, report: &Report) {
        match report {
            Report::Measure(MeasureReport::Start(start)) => match start.kind {
                StartKind::Cold => self.cold = Some(start.clone()),
                StartKind::Warm => self.warm = Some(start.clone()),
            },
            Report::Measure(MeasureReport::Idle(idle)) => self.idle = Some(idle.clone()),
            Report::Measure(MeasureReport::ExeSize { bytes }) => {
                self.exe_size = Some(bytes.clone())
            }
            Report::Index(index) => self.index = Some(index.clone()),
            Report::Machine(machine) => self.machine = Some(*machine),
            Report::Run(run) => self.run = Some(run.clone()),
            Report::Measurement { .. } | Report::Bucket { .. } | Report::Extension { .. } => {
                self.profile.push(report.clone());
            }
            Report::Message { .. }
            | Report::Document(_)
            | Report::Timings(_)
            | Report::Error { .. }
            | Report::Panic { .. } => {}
        }
    }
}
