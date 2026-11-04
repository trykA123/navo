use std::io::Write;

use super::phase::{Observer, Verdict};
use crate::privacy::{Output, Report, SafeText, SafeWriter};
use crate::safe_text;

/// Phase progress on stderr: phase names, verdict words and codes only.
pub struct Progress<W> {
    writer: SafeWriter<W>,
}

impl<W: Write> Progress<W> {
    pub fn new(writer: SafeWriter<W>) -> Self {
        Self { writer }
    }

    fn say(&mut self, report: &Report) {
        // A failed progress line must not stop the measurement.
        let _ = self.writer.write(Output::Console, report);
    }
}

impl<W: Write> Observer for Progress<W> {
    fn started(&mut self, name: SafeText) {
        self.say(&Report::Message { label: name });
    }

    fn finished(&mut self, _name: SafeText, verdict: Verdict) {
        let report = match verdict {
            Verdict::Ok => Report::Measurement {
                label: safe_text!("phase ok"),
                value: 0,
            },
            Verdict::Failed(code) => Report::Measurement {
                label: safe_text!("phase failed, code"),
                value: u64::from(code),
            },
            Verdict::NotMeasured(reason) => Report::Message { label: reason },
        };
        self.say(&report);
    }
}
