use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::{MemoryReport, Redactor};

mod document;
mod summary;

use document::Document;
use summary::{SECTIONS, summary_text};

#[derive(Debug, Clone, Default, Serialize)]
pub struct BuildInfo {
    pub version: String,
    pub commit: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SystemInfo {
    pub os: String,
    pub cpu_count: usize,
    pub ram_bytes: u64,
    pub gpu: String,
    pub scale: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorRecord {
    pub kind: String,
    pub os_code: Option<i32>,
    pub operation: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
pub struct ReportInput {
    pub build: BuildInfo,
    pub system: SystemInfo,
    pub settings_changed: Vec<(String, String)>,
    pub errors: Vec<ErrorRecord>,
    pub events: Vec<Arc<str>>,
    pub memory: Option<MemoryReport>,
    pub startup_ms: Vec<(String, f64)>,
    pub redaction_misses: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportSummary {
    pub sections: Vec<String>,
    pub events: usize,
    pub errors: usize,
    pub bytes: u64,
}

pub fn write_report(
    path: &Path,
    input: &ReportInput,
    redactor: &Redactor,
) -> io::Result<ReportSummary> {
    let report = serde_json::to_vec_pretty(&Document::new(input, redactor))?;
    let summary = summary_text(input);
    if fs::symlink_metadata(path).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "the report file already exists",
        ));
    }
    let temp = temp_path(path)?;
    let file = File::create_new(&temp)?;
    let written = write_zip(file, &report, summary.as_bytes())
        .and_then(|()| publish(&temp, path))
        .and_then(|()| fs::metadata(path));
    let _ = fs::remove_file(&temp);
    let bytes = written?.len();
    Ok(ReportSummary {
        sections: SECTIONS.iter().map(|&s| s.to_owned()).collect(),
        events: input.events.len(),
        errors: input.errors.len(),
        bytes,
    })
}

fn publish(temp: &Path, path: &Path) -> io::Result<()> {
    match fs::hard_link(temp, path) {
        Err(error) if error.kind() != io::ErrorKind::AlreadyExists => {
            if fs::symlink_metadata(path).is_ok() {
                return Err(io::ErrorKind::AlreadyExists.into());
            }
            fs::rename(temp, path)
        }
        other => other,
    }
}

fn temp_path(path: &Path) -> io::Result<PathBuf> {
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "report path has no file name")
    })?;
    let mut temp = name.to_owned();
    temp.push(format!(".{}.tmp", std::process::id()));
    Ok(path.with_file_name(temp))
}

fn write_zip(file: File, report: &[u8], summary: &[u8]) -> io::Result<()> {
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut zip = ZipWriter::new(file);
    zip.start_file("report.json", options)?;
    zip.write_all(report)?;
    zip.start_file("summary.txt", options)?;
    zip.write_all(summary)?;
    zip.finish()?.sync_all()
}

#[cfg(test)]
mod tests;
