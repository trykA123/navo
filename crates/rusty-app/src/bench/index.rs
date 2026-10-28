use std::ffi::OsString;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use rusty_index::pipe::{Client, MemoryReport, Query, QueryTerm, Reply, Request};
use rusty_l10n::t;
use serde::Serialize;

const RUNS: usize = 10;
const TIMING_COMMAND: &str = "cargo nextest run -p rusty-app --run-ignored all -j 1 timing_tests";

#[derive(Debug, Serialize)]
struct Measurement {
    cold_start_ms: Option<f64>,
    warm_start_ms: Option<f64>,
    timing_tests_command: &'static str,
    first_answer_ms: f64,
    first_building_answer_ms: Option<f64>,
    entries: u64,
    building: bool,
    queries: Vec<QueryTiming>,
    memory: Memory,
}

#[derive(Debug, Serialize)]
struct QueryTiming {
    query: &'static str,
    runs: usize,
    min_ms: f64,
    median_ms: f64,
    max_ms: f64,
    results: usize,
}

#[derive(Debug, Serialize)]
struct Memory {
    busy: bool,
    parts: Vec<MemoryPart>,
}

#[derive(Debug, Serialize)]
struct MemoryPart {
    name: &'static str,
    bytes: u64,
    items: u64,
}

pub(crate) fn parse(mut args: impl Iterator<Item = OsString>) -> Result<bool, String> {
    if args.next().as_deref() != Some(std::ffi::OsStr::new("index")) {
        return Err(t!("bench-index-usage"));
    }
    let json = match args.next() {
        None => false,
        Some(flag) if flag == "--json" => true,
        Some(_) => return Err(t!("bench-index-usage")),
    };
    if args.next().is_some() {
        return Err(t!("bench-index-usage"));
    }
    Ok(json)
}

pub(crate) fn run(json: bool) -> ExitCode {
    finish(execute(json, connect))
}

fn finish(result: Result<String, String>) -> ExitCode {
    match result {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("navo --bench index: {}", error.replace(['\r', '\n'], " "));
            ExitCode::FAILURE
        }
    }
}

fn connect() -> Result<Client, String> {
    #[cfg(windows)]
    {
        require_running(
            crate::index_service::query()
                .map_err(|error| t!("bench-index-service-error", error = error.to_string()))?
                .running,
        )?;
        Client::connect()
            .map_err(|error| t!("bench-index-service-error", error = error.to_string()))
    }
    #[cfg(not(windows))]
    {
        Err(t!("bench-index-windows"))
    }
}

#[cfg(any(windows, test))]
fn require_running(running: Option<bool>) -> Result<(), String> {
    match running {
        None => Err(t!("bench-index-not-installed")),
        Some(false) => Err(t!("bench-index-not-running")),
        Some(true) => Ok(()),
    }
}

fn execute(json: bool, connect: impl FnOnce() -> Result<Client, String>) -> Result<String, String> {
    let mut client = connect()?;
    let mut generation = 0;
    let report = measure(
        |request| {
            generation += 1;
            let query = Query {
                request,
                generation,
            };
            let started = Instant::now();
            let reply = client
                .query(&query)
                .map_err(|error| t!("bench-index-query-error", error = error.to_string()))?;
            Ok((reply, started.elapsed()))
        },
        || {
            Client::memory(Instant::now() + rusty_index::pipe::QUERY_TIMEOUT)
                .map_err(|error| t!("bench-index-memory-error", error = error.to_string()))
        },
    )?;
    format_report(&report, json)
}

fn queries() -> [(&'static str, Request); 5] {
    let name = |text: &str| QueryTerm::Name(text.into());
    [
        ("windows", vec![name("windows")]),
        ("win", vec![name("win")]),
        ("*.dll", vec![QueryTerm::Ext(vec!["dll".into()])]),
        ("windows update", vec![name("windows"), name("update")]),
        (
            r"Windows\System32",
            vec![QueryTerm::Path(r"Windows\System32".into())],
        ),
    ]
    .map(|(text, terms)| (text, Request { terms }))
}

fn measure(
    mut query: impl FnMut(Request) -> Result<(Reply, Duration), String>,
    memory: impl FnOnce() -> Result<MemoryReport, String>,
) -> Result<Measurement, String> {
    let (first, elapsed) = query(Request::default())?;
    let mut report = Measurement {
        cold_start_ms: None,
        warm_start_ms: None,
        timing_tests_command: TIMING_COMMAND,
        first_answer_ms: super::report::ms(elapsed),
        first_building_answer_ms: None,
        entries: 0,
        building: false,
        queries: Vec::new(),
        memory: Memory {
            busy: false,
            parts: Vec::new(),
        },
    };
    report.observe(&first, elapsed);
    for fixed in queries() {
        let timing = measure_query(fixed, &mut query, &mut report)?;
        report.queries.push(timing);
    }
    let memory = memory()?;
    report.memory = Memory {
        busy: memory.busy,
        parts: memory
            .known()
            .map(|(part, found)| MemoryPart {
                name: part.label(),
                bytes: found.bytes,
                items: found.items,
            })
            .collect(),
    };
    Ok(report)
}

fn measure_query(
    (text, request): (&'static str, Request),
    query: &mut impl FnMut(Request) -> Result<(Reply, Duration), String>,
    report: &mut Measurement,
) -> Result<QueryTiming, String> {
    let mut samples = [0.0; RUNS];
    let mut results = 0;
    for sample in &mut samples {
        let (reply, elapsed) = query(request.clone())?;
        report.observe(&reply, elapsed);
        *sample = super::report::ms(elapsed);
        results = reply.hits.len();
    }
    samples.sort_by(f64::total_cmp);
    Ok(QueryTiming {
        query: text,
        runs: RUNS,
        min_ms: samples[0],
        median_ms: (samples[RUNS / 2 - 1] + samples[RUNS / 2]) / 2.0,
        max_ms: samples[RUNS - 1],
        results,
    })
}

impl Measurement {
    fn observe(&mut self, reply: &Reply, elapsed: Duration) {
        self.entries = reply
            .status
            .volumes
            .iter()
            .map(|volume| volume.entries)
            .sum();
        self.building = reply
            .status
            .volumes
            .iter()
            .any(|volume| volume.progress.is_some());
        if self.building && self.first_building_answer_ms.is_none() {
            self.first_building_answer_ms = Some(super::report::ms(elapsed));
        }
    }
}

fn format_report(report: &Measurement, json: bool) -> Result<String, String> {
    if json {
        return serde_json::to_string_pretty(report).map_err(|error| error.to_string());
    }
    let mut lines = vec![
        t!("bench-index-heading"),
        t!("bench-index-start", command = TIMING_COMMAND),
        t!(
            "bench-index-first",
            ms = format!("{:.3}", report.first_answer_ms)
        ),
        report.first_building_answer_ms.map_or_else(
            || t!("bench-index-unobserved"),
            |ms| t!("bench-index-building-answer", ms = format!("{ms:.3}")),
        ),
        t!(
            "bench-index-entries",
            count = report.entries.to_string(),
            building = report.building.to_string()
        ),
        t!("bench-index-query-heading", runs = RUNS.to_string()),
    ];
    lines.extend(report.queries.iter().map(|query| {
        t!(
            "bench-index-query",
            query = query.query,
            min = format!("{:.3}", query.min_ms),
            median = format!("{:.3}", query.median_ms),
            max = format!("{:.3}", query.max_ms),
            results = query.results.to_string()
        )
    }));
    lines.push(t!(
        "bench-index-memory",
        busy = report.memory.busy.to_string()
    ));
    lines.extend(report.memory.parts.iter().map(|part| {
        t!(
            "bench-index-memory-part",
            name = part.name,
            bytes = part.bytes.to_string(),
            items = part.items.to_string()
        )
    }));
    Ok(lines.join("\n"))
}

#[cfg(test)]
#[path = "index/tests.rs"]
mod tests;
