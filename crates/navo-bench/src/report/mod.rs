//! The run report: v1.0 targets with verdicts, schema-1 JSON and one self-contained HTML page.
//!
//! Both outputs are built from typed results and written through `SafeWriter`.

mod data;
mod fmt;
mod html;
mod labels;
mod observe;
mod results;
mod safety;
mod sections;
mod style;
mod targets;
mod timings;

use serde::Serialize;

use crate::privacy::{Html, Output, PrivacyError, Report, SafeWriter};

pub use results::Results;
pub use safety::{PhaseOutcome, PhaseRow, Proof, RunSummary};
pub use targets::{
    Observed, Overall, TARGETS, Tally, TargetId, TargetRow, TargetSpec, Unit, Verdict,
};
pub use timings::{TimingFolder, TimingRow, TimingStep, Timings};

pub const SCHEMA_VERSION: u32 = 1;

/// The JSON body: numbers and enums only.
#[derive(Clone, Debug, Serialize)]
pub struct Document {
    pub schema_version: u32,
    pub tally: Tally,
    pub targets: Vec<TargetRow>,
    pub timings: Timings,
    pub reports: Vec<Report>,
}

#[derive(Clone, Debug)]
pub struct RunReport {
    document: Document,
    results: Results,
}

impl RunReport {
    pub fn new(reports: Vec<Report>, timings: Timings) -> Self {
        let results = Results::from_reports(&reports, timings.clone());
        let targets: Vec<TargetRow> = TARGETS
            .iter()
            .map(|spec| TargetRow::judge(spec, observe::observe(spec, &results)))
            .collect();
        let reports = reports
            .into_iter()
            .filter(|report| !matches!(report, Report::Document(_)))
            .collect();
        let document = Document {
            schema_version: SCHEMA_VERSION,
            tally: Tally::of(&targets),
            targets,
            timings,
            reports,
        };
        Self { document, results }
    }

    pub fn tally(&self) -> Tally {
        self.document.tally
    }

    pub fn targets(&self) -> &[TargetRow] {
        &self.document.targets
    }

    pub fn page(&self) -> Html {
        html::page(&self.document, &self.results)
    }

    pub fn write_json<W: std::io::Write>(
        &self,
        writer: &mut SafeWriter<W>,
    ) -> Result<(), PrivacyError> {
        let report = Report::Document(Box::new(self.document.clone()));
        writer.write(Output::Json, &report)
    }

    pub fn write_html<W: std::io::Write>(
        &self,
        writer: &mut SafeWriter<W>,
    ) -> Result<(), PrivacyError> {
        writer.write_html(&self.page())
    }
}

#[cfg(test)]
mod contrast_tests;
#[cfg(test)]
mod html_tests;
#[cfg(test)]
mod tests;
