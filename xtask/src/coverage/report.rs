use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use super::CoverageError;

#[derive(Deserialize)]
struct LineCoverage {
    count: u64,
    covered: u64,
}

pub fn crate_line_coverage_in(
    llvm_cov_json: &str,
    root: &Path,
) -> Result<BTreeMap<String, f64>, CoverageError> {
    let root = root.to_string_lossy().replace('\\', "/");
    if root.is_empty() {
        return super::crate_line_coverage(llvm_cov_json);
    }
    let prefix = format!("{}/", root.trim_end_matches('/'));
    line_coverage(llvm_cov_json, Some(&prefix))
}

pub fn crate_line_coverage(llvm_cov_json: &str) -> Result<BTreeMap<String, f64>, CoverageError> {
    line_coverage(llvm_cov_json, None)
}

fn line_coverage(
    llvm_cov_json: &str,
    root: Option<&str>,
) -> Result<BTreeMap<String, f64>, CoverageError> {
    let report: Value = serde_json::from_str(llvm_cov_json)
        .map_err(|error| CoverageError::InvalidReport(error.to_string()))?;
    let files = report
        .pointer("/data/0/files")
        .and_then(Value::as_array)
        .ok_or_else(|| CoverageError::InvalidReport("missing data[0].files array".to_owned()))?;
    let mut totals = BTreeMap::new();
    for file in files {
        let Some((crate_name, lines)) = file_coverage(file, root)? else {
            continue;
        };
        let (count, covered) = totals.entry(crate_name).or_insert((0.0, 0.0));
        *count += lines.count as f64;
        *covered += lines.covered as f64;
    }
    Ok(totals
        .into_iter()
        .map(|(name, (count, covered))| {
            let percent = if count == 0.0 {
                100.0
            } else {
                100.0 * covered / count
            };
            (name, percent)
        })
        .collect())
}

fn file_coverage(
    file: &Value,
    root: Option<&str>,
) -> Result<Option<(String, LineCoverage)>, CoverageError> {
    let filename = file
        .get("filename")
        .and_then(Value::as_str)
        .ok_or_else(|| CoverageError::InvalidReport("missing file filename".to_owned()))?;
    let normalized = filename.replace('\\', "/");
    let path = match root {
        Some(root) => normalized.strip_prefix(root),
        None => Some(normalized.as_str()),
    };
    let Some(crate_name) = path.and_then(crate_name) else {
        return Ok(None);
    };
    let lines = file.pointer("/summary/lines").ok_or_else(|| {
        CoverageError::InvalidReport(format!("{filename}: missing summary.lines"))
    })?;
    serde_json::from_value(lines.clone())
        .map(|lines| Some((crate_name, lines)))
        .map_err(|error| CoverageError::InvalidReport(format!("{filename}: {error}")))
}

fn crate_name(filename: &str) -> Option<String> {
    let mut segments = filename.split('/');
    while let Some(segment) = segments.next() {
        match segment {
            "crates" => {
                return segments
                    .next()
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned);
            }
            "xtask" => return Some("xtask".to_owned()),
            _ => {}
        }
    }
    None
}
