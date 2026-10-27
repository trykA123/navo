use serde::Serialize;
use serde_json::Value;

use super::{BuildInfo, ErrorRecord, ReportInput, SystemInfo};
use crate::heap::HeapStats;
use crate::{MemoryReport, Redactor};

#[derive(Serialize)]
pub(super) struct Document {
    build: BuildInfo,
    system: SystemInfo,
    configuration: Configuration,
    errors: Vec<ErrorRecord>,
    recent_events: Vec<Value>,
    memory: Option<Memory>,
    performance: Performance,
    redaction_misses: u64,
}

#[derive(Serialize)]
struct Configuration {
    settings_changed: Vec<Setting>,
}

#[derive(Serialize)]
struct Setting {
    key: String,
    value: String,
}

#[derive(Serialize)]
struct Memory {
    process: Option<Process>,
    probe_error: Option<String>,
    caches: Vec<Cache>,
    tasks: Vec<Task>,
    rust_heap: Option<HeapStats>,
    components: Vec<Component>,
}

#[derive(Serialize)]
struct Component {
    name: String,
    bytes: u64,
    items: u64,
    busy: bool,
}

#[derive(Serialize)]
struct Process {
    resident_bytes: u64,
    private_bytes: u64,
}

#[derive(Serialize)]
struct Cache {
    name: String,
    bytes: u64,
    pinned_bytes: u64,
    unpinned_bytes: u64,
    entries: u64,
    budget: u64,
}

#[derive(Serialize)]
struct Task {
    owner: String,
    count: usize,
}

#[derive(Serialize)]
struct Performance {
    startup_ms: Vec<Mark>,
}

#[derive(Serialize)]
struct Mark {
    name: String,
    ms: f64,
}

impl Document {
    pub(super) fn new(input: &ReportInput, redactor: &Redactor) -> Self {
        let settings_changed = input
            .settings_changed
            .iter()
            .map(|(key, value)| Setting {
                key: redactor.text(key),
                value: redactor.text(value),
            })
            .collect();
        Self {
            build: build(&input.build, redactor),
            system: system(&input.system, redactor),
            configuration: Configuration { settings_changed },
            errors: input.errors.iter().map(|e| error(e, redactor)).collect(),
            recent_events: input.events.iter().map(|e| event(e, redactor)).collect(),
            memory: input.memory.as_ref().map(|m| memory(m, redactor)),
            performance: performance(&input.startup_ms, redactor),
            redaction_misses: input.redaction_misses,
        }
    }
}

fn build(build: &BuildInfo, redactor: &Redactor) -> BuildInfo {
    BuildInfo {
        version: redactor.text(&build.version),
        commit: redactor.text(&build.commit),
    }
}

fn system(system: &SystemInfo, redactor: &Redactor) -> SystemInfo {
    SystemInfo {
        os: redactor.text(&system.os),
        gpu: redactor.text(&system.gpu),
        ..system.clone()
    }
}

fn error(record: &ErrorRecord, redactor: &Redactor) -> ErrorRecord {
    ErrorRecord {
        kind: redactor.text(&record.kind),
        os_code: record.os_code,
        operation: redactor.text(&record.operation),
        detail: redactor.text(&record.detail),
    }
}

fn event(line: &str, redactor: &Redactor) -> Value {
    match serde_json::from_str::<Value>(line.trim_end()) {
        Ok(value) => redact_value(value, redactor),
        Err(_) => Value::String(redactor.text(line)),
    }
}

fn redact_value(value: Value, redactor: &Redactor) -> Value {
    match value {
        Value::String(text) => Value::String(redactor.text(&text)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| redact_value(item, redactor))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, item)| (redactor.text(&key), redact_value(item, redactor)))
                .collect(),
        ),
        other => other,
    }
}

fn memory(report: &MemoryReport, redactor: &Redactor) -> Memory {
    Memory {
        process: report.process.map(|p| Process {
            resident_bytes: p.resident_bytes,
            private_bytes: p.private_bytes,
        }),
        probe_error: report.probe_error.as_deref().map(|e| redactor.text(e)),
        caches: report
            .caches
            .iter()
            .map(|c| Cache {
                name: redactor.text(&c.name),
                bytes: c.size.bytes,
                pinned_bytes: c.size.pinned_bytes,
                unpinned_bytes: c.size.unpinned_bytes(),
                entries: c.size.entries,
                budget: c.budget,
            })
            .collect(),
        tasks: report
            .tasks
            .iter()
            .map(|(owner, count)| Task {
                owner: redactor.text(owner),
                count: *count,
            })
            .collect(),
        rust_heap: report.heap,
        components: report
            .components
            .iter()
            .map(|c| Component {
                name: redactor.text(c.name),
                bytes: c.bytes,
                items: c.items,
                busy: c.busy,
            })
            .collect(),
    }
}

fn performance(marks: &[(String, f64)], redactor: &Redactor) -> Performance {
    let startup_ms = marks
        .iter()
        .map(|(name, ms)| Mark {
            name: redactor.text(name),
            ms: *ms,
        })
        .collect();
    Performance { startup_ms }
}
