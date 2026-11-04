use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use super::phases::Outcome;
use crate::guard::{GuardError, OutputFolder, StaleCleanup};
use crate::harness::start::StartError;
use crate::privacy::{LeakScanner, PrivacyError, Report, SafeText, SafeWriter, SafetyVerdict};
use crate::report::RunReport;
use crate::safe_text;

pub(super) const JSON_NAME: &str = "report.json";
pub(super) const HTML_NAME: &str = "report.html";

pub(super) const USAGE: SafeText =
    safe_text!("usage: navo-bench [--folder <folder>] [--quick] [--out <new folder>]");
pub(super) const OUT_REFUSED: SafeText =
    safe_text!("the output folder was refused: --out must name a new folder whose parent exists");
pub(super) const CLEANUP_FAILED: SafeText =
    safe_text!("the run folder could not be moved to the Recycle Bin; the next run retries");
pub(super) const FOLDER_UNREADABLE: SafeText = safe_text!("the chosen folder could not be read");

/// Console lines through `SafeWriter`: fixed text, codes and counts only.
pub(super) struct Console<W> {
    writer: SafeWriter<W>,
}

impl<W: Write> Console<W> {
    pub(super) fn new(sink: W, scanner: LeakScanner) -> Self {
        Self {
            writer: SafeWriter::new(sink, scanner),
        }
    }

    fn say(&mut self, report: &Report) {
        // A lost console line must not stop the run.
        let _ = self.writer.write(crate::privacy::Output::Console, report);
    }

    pub(super) fn line(&mut self, label: SafeText) {
        self.say(&Report::Message { label });
    }

    /// The data-safety verdict, which no protected name can redact away.
    pub(super) fn verdict(&mut self, verdict: SafetyVerdict) {
        // A lost console line must not stop the run; the exit code still states it.
        let _ = self.writer.write_verdict(verdict);
    }

    pub(super) fn count(&mut self, label: SafeText, value: u64) {
        self.say(&Report::Measurement { label, value });
    }

    pub(super) fn start_error(&mut self, error: StartError) {
        match error {
            StartError::Cancelled => {
                self.line(safe_text!(
                    "no folder chosen; the folder checks are skipped"
                ));
            }
            StartError::PathRequired => self.line(USAGE),
            StartError::Picker(_) => self.line(safe_text!("the folder picker failed")),
            StartError::Refused(error) => {
                self.line(safe_text!("this folder cannot be measured"));
                self.say(&Report::Error { error });
            }
        }
    }

    pub(super) fn guard_error(&mut self, label: SafeText, error: GuardError) {
        self.line(label);
        self.line(match error {
            GuardError::Exists(_) => safe_text!("it already exists"),
            GuardError::ForbiddenParent(_) => {
                safe_text!("its parent is a drive root or a system or user-profile folder")
            }
            GuardError::OwnerOverlap(_) => safe_text!("it overlaps the folder being measured"),
            GuardError::InvalidPath(_) | GuardError::ReparsePoint(_) => {
                safe_text!("the path is not a plain folder path")
            }
            _ => safe_text!("the disk refused it"),
        });
    }

    pub(super) fn stale(&mut self, stale: StaleCleanup) {
        if stale.recycled > 0 {
            let label = safe_text!("old run folders moved to the Recycle Bin");
            self.count(label, u64::try_from(stale.recycled).unwrap_or(u64::MAX));
        }
    }
}

/// Builds the report and writes report.json and report.html into the output folder; true
/// when both were written.
pub(super) fn publish(
    outcome: Outcome,
    out: &OutputFolder,
    console: &mut impl Write,
    scanner: &LeakScanner,
) -> bool {
    let verdict = outcome.verdict;
    let report = RunReport::new(outcome.reports, outcome.timings);
    let written = write_file(out, JSON_NAME, |file| {
        report.write_json(&mut report_writer(file, scanner, verdict))
    })
    .and_then(|()| {
        write_file(out, HTML_NAME, |file| {
            report.write_html(&mut report_writer(file, scanner, verdict))
        })
    });
    let mut say = Console::new(console, scanner.clone());
    if written.is_some() {
        say.line(safe_text!(
            "report.json and report.html are in the output folder"
        ));
    } else {
        say.line(safe_text!("the report could not be written"));
    }
    written.is_some()
}

/// A redacted report file still ends with the data-safety verdict.
fn report_writer<'a>(
    file: &'a mut File,
    scanner: &LeakScanner,
    verdict: SafetyVerdict,
) -> SafeWriter<&'a mut File> {
    SafeWriter::new(file, scanner.clone()).with_verdict(verdict)
}

fn write_file(
    out: &OutputFolder,
    name: &str,
    write: impl FnOnce(&mut File) -> Result<(), PrivacyError>,
) -> Option<()> {
    let mut file = out.create_report(Path::new(name)).ok()?;
    write(&mut file).ok()?;
    file.sync_all().ok()
}

/// `navo-bench-<yyyymmdd>-<hhmmss>` in UTC.
pub(super) fn default_name(now: SystemTime) -> String {
    let seconds = now
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let (year, month, day) = civil_date(seconds / 86_400);
    let time = seconds % 86_400;
    format!(
        "navo-bench-{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        time / 3600,
        time % 3600 / 60,
        time % 60
    )
}

/// Days since 1970-01-01 to a Gregorian date (H. Hinnant's `civil_from_days`).
fn civil_date(days: u64) -> (u64, u64, u64) {
    let shifted = days + 719_468;
    let era = shifted / 146_097;
    let day_of_era = shifted % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
}
