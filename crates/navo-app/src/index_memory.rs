use std::process::ExitCode;
use std::time::Instant;

use navo_index::pipe::{Client, MemoryReport, PipeError, QUERY_TIMEOUT, VolumeMemory};
use serde::Serialize;

#[derive(Serialize)]
struct Sample<'a> {
    service_process_id: Option<u32>,
    parts: Vec<Part>,
    volumes: &'a Option<Vec<VolumeMemory>>,
    busy: bool,
    time: &'a str,
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
        busy: report.busy,
        time,
    })
}

#[cfg(test)]
#[path = "index_memory/tests.rs"]
mod tests;
