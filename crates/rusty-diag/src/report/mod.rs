use std::path::Path;
use std::sync::Arc;

use serde::Serialize;

use crate::{MemoryReport, Redactor};

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
    _path: &Path,
    _input: &ReportInput,
    _redactor: &Redactor,
) -> std::io::Result<ReportSummary> {
    todo!()
}

#[cfg(test)]
mod tests;
