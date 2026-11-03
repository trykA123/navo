use std::process::ExitCode;
use std::time::Instant;

use navo_index::pipe::{
    Client, MemoryReport, PipeError, QUERY_TIMEOUT, VolumeBuilds, VolumeMemory,
};
use serde::Serialize;

#[derive(Serialize)]
struct Sample<'a> {
    service_process_id: Option<u32>,
    parts: Vec<Part>,
    volumes: &'a Option<Vec<VolumeMemory>>,
    builds: Option<Vec<Builds>>,
    busy: bool,
    time: &'a str,
}

#[derive(Serialize)]
struct Builds {
    full_builds_since_start: u64,
    last_build_reason: Option<&'static str>,
    last_build_duration_ms: u64,
    unresolved_batches: u64,
    deferred_rebuilds: u64,
    dropped_changes: u64,
}

impl From<&VolumeBuilds> for Builds {
    fn from(builds: &VolumeBuilds) -> Self {
        Self {
            full_builds_since_start: builds.full_builds,
            last_build_reason: builds.last_reason.map(|reason| reason.label()),
            last_build_duration_ms: builds.last_duration_ms,
            unresolved_batches: builds.unresolved_batches,
            deferred_rebuilds: builds.deferred_rebuilds,
            dropped_changes: builds.dropped_changes,
        }
    }
}

#[derive(Serialize)]
struct Part {
    name: &'static str,
    bytes: u64,
    items: u64,
}

pub(crate) fn run() -> ExitCode {
    finish(Client::memory(Instant::now() + QUERY_TIMEOUT))
}

fn finish(result: Result<MemoryReport, PipeError>) -> ExitCode {
    let report = match result {
        Ok(report) => report,
        Err(PipeError::NotAvailable) => {
            eprintln!("navo --index-memory: index service is not running");
            return ExitCode::from(2);
        }
        Err(_) => {
            eprintln!("navo --index-memory: cannot read index service memory");
            return ExitCode::FAILURE;
        }
    };
    match format_sample(&report, &chrono::Utc::now().to_rfc3339()) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(_) => {
            eprintln!("navo --index-memory: cannot encode memory sample");
            ExitCode::FAILURE
        }
    }
}

fn format_sample(report: &MemoryReport, time: &str) -> Result<String, serde_json::Error> {
    serde_json::to_string(&Sample {
        service_process_id: report.process_id,
        parts: report
            .known()
            .map(|(part, found)| Part {
                name: part.label(),
                bytes: found.bytes,
                items: found.items,
            })
            .collect(),
        volumes: &report.volumes,
        builds: report
            .builds
            .as_ref()
            .map(|builds| builds.iter().map(Builds::from).collect()),
        busy: report.busy,
        time,
    })
}

#[cfg(test)]
#[path = "index_memory/tests.rs"]
mod tests;
