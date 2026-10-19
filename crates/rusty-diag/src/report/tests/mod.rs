use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;
use tempfile::TempDir;

use super::{BuildInfo, ErrorRecord, ReportInput, ReportSummary, SystemInfo, write_report};
use crate::{
    CacheSize, CacheUsage, DpiAwareness, Identity, MemoryReport, ProcessMemory, Redactor, Salt,
};

mod canary;
mod report;

const SECTIONS: [&str; 7] = [
    "build",
    "system",
    "configuration",
    "errors",
    "recent_events",
    "memory",
    "performance",
];

fn identity() -> Identity {
    Identity {
        user: Some("canary-user-7731".to_owned()),
        computer: Some("CANARY-PC-7731".to_owned()),
        domain: Some("CANARYDOM".to_owned()),
        profile: Some(r"C:\Users\canary-user-7731".to_owned()),
    }
}

fn redactor() -> Redactor {
    Redactor::new(Salt::from_bytes([7; 16]), identity())
}

fn error(operation: &str, detail: &str) -> ErrorRecord {
    ErrorRecord {
        kind: "PermissionDenied".to_owned(),
        os_code: Some(5),
        operation: operation.to_owned(),
        detail: detail.to_owned(),
    }
}

fn memory() -> MemoryReport {
    MemoryReport {
        process: Some(ProcessMemory {
            resident_bytes: 123_456_789,
            private_bytes: 98_765_432,
        }),
        probe_error: None,
        caches: vec![
            CacheUsage {
                name: "listings in memory".to_owned(),
                size: CacheSize {
                    bytes: 70_254_592,
                    pinned_bytes: 3_145_728,
                    entries: 1_204,
                },
                budget: 67_108_864,
            },
            CacheUsage {
                name: "thumbnails".to_owned(),
                size: CacheSize {
                    bytes: 4_096,
                    pinned_bytes: 0,
                    entries: 3,
                },
                budget: 65_536,
            },
        ],
        tasks: vec![("listing".to_owned(), 2)],
    }
}

fn input() -> ReportInput {
    ReportInput {
        build: BuildInfo {
            version: "0.0.1".to_owned(),
            commit: "38b48d7".to_owned(),
        },
        system: SystemInfo {
            os: "Windows 11 26100".to_owned(),
            cpu_count: 8,
            ram_bytes: 17_179_869_184,
            gpu: "integrated".to_owned(),
            scale: 1.5,
            dpi_awareness: DpiAwareness::PerMonitorV2,
        },
        settings_changed: vec![("theme".to_owned(), "dark".to_owned())],
        errors: vec![error("list folder", "access denied")],
        events: vec![
            Arc::from(r#"{"t":1,"level":"INFO","target":"ferrite","msg":"start","fields":{}}"#),
            Arc::from(
                r#"{"t":2,"level":"WARN","target":"ferrite","msg":"slow","fields":{"ms":412}}"#,
            ),
        ],
        memory: Some(memory()),
        startup_ms: vec![
            ("window".to_owned(), 12.5),
            ("first_paint".to_owned(), 48.25),
        ],
        redaction_misses: 3,
    }
}

struct Written {
    _dir: TempDir,
    path: PathBuf,
    summary: ReportSummary,
}

fn write(input: &ReportInput) -> Written {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("report.zip");
    let summary = write_report(&path, input, &redactor()).unwrap();
    Written {
        _dir: dir,
        path,
        summary,
    }
}

fn entries(path: &Path) -> Vec<(String, Vec<u8>)> {
    let mut archive = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
    (0..archive.len())
        .map(|index| {
            let mut file = archive.by_index(index).unwrap();
            let name = file.name().unwrap().into_owned();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            (name, bytes)
        })
        .collect()
}

fn entry(path: &Path, name: &str) -> String {
    let (_, bytes) = entries(path)
        .into_iter()
        .find(|(entry, _)| entry == name)
        .unwrap();
    String::from_utf8(bytes).unwrap()
}

fn report_text(path: &Path) -> String {
    entry(path, "report.json")
}

fn report_json(path: &Path) -> Value {
    serde_json::from_str(&report_text(path)).unwrap()
}

fn summary_text(path: &Path) -> String {
    entry(path, "summary.txt")
}

fn file_size(path: &Path) -> u64 {
    fs::metadata(path).unwrap().len()
}
