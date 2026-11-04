mod crash;
mod error;
mod extension;
mod html;
mod scan;
mod verdict;

use std::io::Write;

use serde::Serialize;

pub use crash::{SourceLocation, install_panic_hook};
pub use error::{ErrorCode, PathRole, PrivacyError};
pub use extension::{FoldedExtension, fold_extension};
pub use html::Html;
pub use scan::{LeakScanner, assert_no_leaks};
pub use verdict::SafetyVerdict;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct SafeText(&'static str);

pub trait StaticLabel {
    const TEXT: &'static str;
}

impl SafeText {
    pub const fn as_str(self) -> &'static str {
        self.0
    }

    pub const fn from_static<L: StaticLabel>() -> Self {
        Self(L::TEXT)
    }
}

#[macro_export]
macro_rules! safe_text {
    ($text:expr) => {{
        struct FixedLabel;
        impl $crate::privacy::StaticLabel for FixedLabel {
            const TEXT: &'static str = $text;
        }
        $crate::privacy::SafeText::from_static::<FixedLabel>()
    }};
}

#[derive(Clone, Copy, Debug)]
pub enum Label {
    WindowsOnly,
    Files,
    Depth,
    ElapsedMicros,
}

impl From<Label> for SafeText {
    fn from(label: Label) -> Self {
        Self(match label {
            Label::WindowsOnly => "navo-bench runs on Windows only",
            Label::Files => "files",
            Label::Depth => "depth",
            Label::ElapsedMicros => "elapsed-micros",
        })
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Report {
    Message {
        label: SafeText,
    },
    Measurement {
        label: SafeText,
        value: u64,
    },
    Bucket {
        group: SafeText,
        bucket: SafeText,
        files: u64,
        bytes: u64,
    },
    Extension {
        extension: FoldedExtension,
        files: u64,
        bytes: u64,
    },
    Index(crate::index::IndexReport),
    Document(Box<crate::report::Document>),
    Machine(crate::machine::MachineSummary),
    Measure(crate::measure::MeasureReport),
    Timings(crate::timings::TimingsReport),
    Run(crate::report::RunSummary),
    Error {
        error: PrivacyError,
    },
    Panic {
        source: SourceLocation,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum Output {
    Html,
    Json,
    Console,
    Log,
    Trace,
    Crash,
}

pub struct SafeWriter<W> {
    sink: W,
    scanner: LeakScanner,
    verdict: Option<SafetyVerdict>,
}

impl<W> std::fmt::Debug for SafeWriter<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SafeWriter").finish_non_exhaustive()
    }
}

impl<W: Write> SafeWriter<W> {
    pub fn new(sink: W, scanner: LeakScanner) -> Self {
        Self {
            sink,
            scanner,
            verdict: None,
        }
    }

    /// A redacted output then still ends with `verdict`.
    #[must_use]
    pub fn with_verdict(mut self, verdict: SafetyVerdict) -> Self {
        self.verdict = Some(verdict);
        self
    }

    /// Writes the verdict line; the count is scanned with its words, the bare label never is.
    pub fn write_verdict(&mut self, verdict: SafetyVerdict) -> Result<(), PrivacyError> {
        let line = verdict.line(&self.scanner);
        self.put(&line)
    }

    pub fn write(&mut self, output: Output, report: &Report) -> Result<(), PrivacyError> {
        let json = serde_json::to_string(report)
            .map_err(|_| PrivacyError::output(ErrorCode::Serialization))?;
        let bytes = encode(output, report, &json);
        self.emit(bytes, &[json.as_bytes()])
    }

    /// Writes a finished page built with `Html`, scanned like every other output; the text
    /// inputs are scanned as given and the page after escaping.
    pub fn write_html(&mut self, page: &Html) -> Result<(), PrivacyError> {
        let sources: Vec<&[u8]> = page.raw_texts().collect();
        self.emit(page.as_bytes().to_vec(), &sources)
    }

    fn emit(&mut self, mut bytes: Vec<u8>, sources: &[&[u8]]) -> Result<(), PrivacyError> {
        let scanner = &self.scanner;
        let count = sources
            .iter()
            .map(|source| scanner.count_leaks(source))
            .fold(scanner.count_leaks(&bytes), usize::max);
        if count > 0 {
            bytes = format!("{count}\n").into_bytes();
            if self.scanner.count_leaks(&bytes) > 0 {
                bytes.clear();
            }
            if let Some(verdict) = self.verdict {
                bytes.extend(verdict.line(&self.scanner));
            }
            if bytes.is_empty() {
                return Err(PrivacyError::output(ErrorCode::Redacted));
            }
        }
        self.put(&bytes)
    }

    fn put(&mut self, bytes: &[u8]) -> Result<(), PrivacyError> {
        self.sink
            .write_all(bytes)
            .and_then(|()| self.sink.flush())
            .map_err(|error| PrivacyError::from_io(PathRole::ScratchRoot, error))
    }
}

fn encode(output: Output, report: &Report, json: &str) -> Vec<u8> {
    if matches!(output, Output::Crash)
        && let Report::Panic { source } = report
    {
        return format!("navo-bench panic {source}\n").into_bytes();
    }
    if matches!(output, Output::Html) {
        let escaped = json
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        format!("<pre>{escaped}</pre>\n").into_bytes()
    } else {
        format!("{json}\n").into_bytes()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LogLevel {
    #[default]
    Off,
    Events,
    Trace,
}

#[derive(Debug)]
pub struct SafeLogger<W> {
    writer: SafeWriter<W>,
    level: LogLevel,
}

impl<W: Write> SafeLogger<W> {
    pub fn new(writer: SafeWriter<W>) -> Self {
        Self {
            writer,
            level: LogLevel::default(),
        }
    }

    pub fn set_level(&mut self, level: LogLevel) {
        self.level = level;
    }

    pub fn event(&mut self, report: &Report) -> Result<(), PrivacyError> {
        if self.level != LogLevel::Off {
            self.writer.write(Output::Log, report)?;
        }
        Ok(())
    }

    pub fn trace(&mut self, report: &Report) -> Result<(), PrivacyError> {
        if self.level == LogLevel::Trace {
            self.writer.write(Output::Trace, report)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod api_tests;
#[cfg(test)]
mod tests;
