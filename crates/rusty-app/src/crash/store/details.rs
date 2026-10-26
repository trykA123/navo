use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::Path;

use rusty_core::{Location, OperationPlan, Step};
use rusty_store::{QuarantineReport, SalvageReport, UnrecoveredJob};
use serde::{Deserialize, Serialize};

use super::{Error, files, state};
use rusty_core::brand;

#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct Header {
    format: String,
    version: u32,
    case: u64,
    attempt: u64,
}

fn header(case: u64, attempt: u64) -> Header {
    Header {
        format: format!("{}-recovery", brand::SLUG),
        version: 1,
        case,
        attempt,
    }
}

pub(super) fn create(
    data: &Path,
    (case, attempt): (u64, u64),
    source: &QuarantineReport,
    report: &SalvageReport,
) -> Result<(), Error> {
    let mut bytes = serde_json::to_vec(&header(case, attempt))?;
    bytes.push(b'\n');
    let readable = report
        .jobs
        .iter()
        .map(|job| serde_json::json!({ "job": job, "paths": paths(job) }))
        .collect::<Vec<_>>();
    let retained = report
        .retained
        .iter()
        .map(|entry| serde_json::json!({ "id": entry.id, "paths": retained_paths(entry) }))
        .collect::<Vec<_>>();
    serde_json::to_writer(
        &mut bytes,
        &serde_json::json!({ "event": "salvaged", "source": source, "report": report, "manual_jobs": readable, "manual_retained": retained }),
    )?;
    bytes.push(b'\n');
    files::create(&state::details(data, case, attempt), &bytes)?;
    Ok(())
}

pub(super) fn recovered(
    data: &Path,
    notice: &state::Notice,
    remaining: &[crate::jobs::StillPending],
) -> Result<(), Error> {
    let path = state::details(data, notice.case, notice.attempt);
    if !files::regular(&path)? {
        return Err(Error::Invalid("the recovery details are missing".into()));
    }
    let mut file = OpenOptions::new().read(true).append(true).open(&path)?;
    file.lock()?;
    let mut prefix = Vec::new();
    Read::by_ref(&mut file).take(512).read_to_end(&mut prefix)?;
    let first = prefix
        .split(|byte| *byte == b'\n')
        .next()
        .unwrap_or_default();
    if serde_json::from_slice::<Header>(first)? != header(notice.case, notice.attempt) {
        return Err(Error::Invalid("the recovery detail header changed".into()));
    }
    let pending = remaining.iter().map(|job| serde_json::json!({ "job": job.job.0, "paths": job.left.iter().map(Location::as_str).collect::<Vec<_>>() })).collect::<Vec<_>>();
    let mut bytes = serde_json::to_vec(
        &serde_json::json!({ "event": "recovered", "restored": notice.recovered_jobs, "pending": pending }),
    )?;
    bytes.push(b'\n');
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

pub(super) fn manual_paths(report: &SalvageReport) -> Vec<String> {
    report
        .jobs
        .iter()
        .flat_map(paths)
        .chain(report.retained.iter().flat_map(retained_paths))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn retained_paths(entry: &rusty_store::UnrecoveredRetained) -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(bytes) = &entry.record
        && let Ok(record) = serde_json::from_slice::<rusty_core::retention::RetentionRecord>(bytes)
    {
        paths.push(record.source.as_str().to_owned());
        paths.push(record.container.as_str().to_owned());
    }
    if let Some(bytes) = &entry.target
        && let Ok(target) = serde_json::from_slice::<rusty_core::retention::RestoreTarget>(bytes)
    {
        paths.push(target.location.as_str().to_owned());
    }
    paths
}

fn paths(job: &UnrecoveredJob) -> Vec<String> {
    let mut paths = BTreeSet::new();
    if let Some(bytes) = &job.plan
        && let Ok(plan) = serde_json::from_slice::<OperationPlan>(bytes)
    {
        for step in &plan.steps {
            match step {
                Step::CopyFile { source, target, .. } | Step::Rename { source, target } => {
                    paths.insert(source.as_str().to_owned());
                    paths.insert(target.as_str().to_owned());
                }
                Step::MakeLink { target_of, at } => {
                    paths.insert(target_of.as_str().to_owned());
                    paths.insert(at.as_str().to_owned());
                }
                Step::MakeFolder { target }
                | Step::RemoveFile { location: target }
                | Step::RemoveFolder { location: target }
                | Step::RemoveLink { location: target }
                | Step::Recycle { location: target } => {
                    paths.insert(target.as_str().to_owned());
                }
            }
        }
    }
    for bytes in &job.held {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
            continue;
        };
        for key in ["original", "held", "placed_target"] {
            let Some(value) = value.get(key) else {
                continue;
            };
            let Ok(location) = serde_json::from_value::<Location>(value.clone()) else {
                continue;
            };
            paths.insert(location.as_str().to_owned());
        }
    }
    paths.into_iter().collect()
}
