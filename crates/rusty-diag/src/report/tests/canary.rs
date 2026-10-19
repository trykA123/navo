use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;

use proptest::prelude::*;
use tempfile::TempDir;
use tracing_subscriber::Registry;
use tracing_subscriber::layer::SubscriberExt;

use super::{BuildInfo, ErrorRecord, ReportInput, SystemInfo, error, redactor, write_report};
use crate::{
    CacheRegistry, CacheUsage, EventRing, FakeProbe, LogConfig, LogLayer, MemoryReport,
    TaskCounters, memory_report,
};

const TARGET: &str = "ferrite::session";
const NEEDLES: [&str; 5] = [
    "canary",
    "7731",
    "canarydom",
    "secret plan",
    "secret plan.docx",
];

const MESSAGES: [&str; 5] = [
    r"session start user=canary-user-7731 host=CANARY-PC-7731 domain=CANARYDOM",
    r"open folder C:\Users\canary-user-7731\Documents\canary-project-7731",
    r"listing \\canary-srv-7731\share\Reports\2026 took 412 ms (network)",
    r"lock owner: CANARYDOM\canary-user-7731 (pid 4412)",
    r"shutdown: CANARY-PC-7731 idle, canary-user-7731 logged off",
];

const PROFILE_PATH: &str = r"C:\Users\canary-user-7731\Music\canary-mix-7731.flac";
const UNC_PATH: &str = r"\\canary-srv-7731\share\canary-7731\q3.xlsx";

const POOL: [&str; 12] = [
    "canary-user-7731",
    "CANARY-PC-7731",
    "CANARYDOM",
    r"CANARYDOM\canary-user-7731",
    r"C:\Users\canary-user-7731\Documents\canary-notes-7731.txt",
    r"c:/users/canary-user-7731/.config/ferrite",
    r"\\canary-srv-7731\share\canary-7731\q3.xlsx",
    r"\\CANARY-PC-7731\c$\Users\canary-user-7731",
    "theme",
    "dark",
    "list folder",
    "42",
];

fn emit(round: usize) {
    let msg = MESSAGES[round % MESSAGES.len()];
    match round % 7 {
        0 => {
            tracing::info!(target: TARGET, user = "canary-user-7731", host = %"CANARY-PC-7731", "{msg}")
        }
        1 => tracing::warn!(target: TARGET, path = PROFILE_PATH, dest_path = %UNC_PATH, "{msg}"),
        2 => {
            tracing::debug!(target: TARGET, detail = PROFILE_PATH, owner = ?r"CANARYDOM\canary-user-7731", "{msg}")
        }
        3 => {
            tracing::error!(target: TARGET, share = UNC_PATH, domain = "CANARYDOM", code = 5, "{msg}")
        }
        4 => tracing::info!(target: TARGET, query = "canary secret plan", hits = 3, "search done"),
        5 => tracing::warn!(target: TARGET, "cannot rename 'Secret Plan.docx'"),
        _ => {
            tracing::trace!(target: TARGET, source_path = PROFILE_PATH, "{msg} {PROFILE_PATH} {UNC_PATH}")
        }
    }
}

fn session_memory() -> MemoryReport {
    let caches = CacheRegistry::default();
    let thumbs = caches.register("thumbs for canary-user-7731", 65_536);
    thumbs.set(4_096);
    let tasks = TaskCounters::default();
    let _scan = tasks.start(r"scan \\canary-srv-7731\share");
    let probe = FakeProbe(Err(io::Error::other(
        r"cannot read C:\Users\canary-user-7731\AppData\canary.tmp",
    )));
    memory_report(&probe, &caches, &tasks)
}

fn session_input(events: Vec<Arc<str>>, redaction_misses: u64) -> ReportInput {
    ReportInput {
        build: BuildInfo {
            version: "0.0.1".to_owned(),
            commit: "38b48d7".to_owned(),
        },
        system: SystemInfo {
            os: "Windows 11 on CANARY-PC-7731 (CANARYDOM)".to_owned(),
            cpu_count: 8,
            ram_bytes: 17_179_869_184,
            gpu: r"integrated, driver C:\Users\canary-user-7731\gpu".to_owned(),
            scale: 1.25,
            dpi_awareness: crate::DpiAwareness::PerMonitorV2,
        },
        settings_changed: vec![
            (
                "start_folder".to_owned(),
                r"C:\Users\canary-user-7731\Documents\canary-plans-7731".to_owned(),
            ),
            ("pinned".to_owned(), UNC_PATH.to_owned()),
            ("CANARY-PC-7731.window".to_owned(), "maximised".to_owned()),
        ],
        errors: vec![
            error(
                &format!("list {UNC_PATH}"),
                r"access denied for CANARYDOM\canary-user-7731",
            ),
            error("rename", "cannot rename 'Secret Plan.docx'"),
        ],
        events,
        memory: Some(session_memory()),
        startup_ms: vec![("window".to_owned(), 10.5)],
        redaction_misses,
    }
}

fn haystacks(zip: &Path, logs: &Path) -> Vec<(String, Vec<u8>)> {
    let mut found = super::entries(zip);
    let names: Vec<String> = found.iter().map(|(name, _)| name.clone()).collect();
    found.push(("zip entry names".to_owned(), names.join("\n").into_bytes()));
    found.push(("zip bytes".to_owned(), fs::read(zip).unwrap()));
    for log in fs::read_dir(logs).unwrap() {
        let log = log.unwrap().path();
        found.push((log.display().to_string(), fs::read(&log).unwrap()));
    }
    found
}

fn leaks(haystacks: &[(String, Vec<u8>)]) -> Vec<String> {
    haystacks
        .iter()
        .flat_map(|(name, bytes)| {
            let text = String::from_utf8_lossy(bytes).to_lowercase();
            NEEDLES
                .iter()
                .filter(move |needle| text.contains(*needle))
                .map(move |needle| format!("{needle} in {name}"))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn canary_values_never_reach_the_logs_or_the_report() {
    let dir = TempDir::new().unwrap();
    let logs = dir.path().join("logs");
    fs::create_dir(&logs).unwrap();
    let redactor = Arc::new(redactor());
    let ring = Arc::new(EventRing::new(EventRing::DEFAULT_BUDGET));
    let config = LogConfig {
        dir: Some(logs.clone()),
        file_stem: "ferrite".to_owned(),
        max_file_bytes: 10 * 1024 * 1024,
        files: 3,
    };
    let layer = LogLayer::new(config, Arc::clone(&redactor), Arc::clone(&ring)).unwrap();
    let misses = layer.misses();
    tracing::subscriber::with_default(Registry::default().with(layer), || {
        (0..25).for_each(emit);
    });
    let zip = dir.path().join("report.zip");
    let input = session_input(ring.snapshot(), misses.count());
    write_report(&zip, &input, &redactor).unwrap();
    let found = leaks(&haystacks(&zip, &logs));
    assert!(found.is_empty(), "canary leaked: {found:#?}");
}

fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(&POOL[..]), 1..4).prop_map(|parts| parts.join(" "))
}

fn event() -> impl Strategy<Value = Arc<str>> {
    (text(), any::<bool>()).prop_map(|(text, json)| {
        let line = if json {
            serde_json::json!({ "msg": text, "fields": { "detail": text } }).to_string()
        } else {
            text
        };
        Arc::from(line)
    })
}

fn record() -> impl Strategy<Value = ErrorRecord> {
    (text(), text(), text()).prop_map(|(kind, operation, detail)| ErrorRecord {
        kind,
        os_code: Some(5),
        operation,
        detail,
    })
}

fn memory() -> impl Strategy<Value = MemoryReport> {
    (text(), text(), text()).prop_map(|(cache, owner, probe_error)| MemoryReport {
        process: None,
        probe_error: Some(probe_error),
        caches: vec![CacheUsage {
            name: cache,
            bytes: 1,
            budget: 2,
        }],
        tasks: vec![(owner, 1)],
    })
}

prop_compose! {
    fn report_input()(
        build in (text(), text()),
        system in (text(), text()),
        settings in prop::collection::vec((text(), text()), 0..4),
        errors in prop::collection::vec(record(), 0..3),
        events in prop::collection::vec(event(), 0..6),
        memory in prop::option::of(memory()),
        startup in prop::collection::vec(text(), 0..3),
    ) -> ReportInput {
        ReportInput {
            build: BuildInfo { version: build.0, commit: build.1 },
            system: SystemInfo { os: system.0, gpu: system.1, ..SystemInfo::default() },
            settings_changed: settings,
            errors,
            events,
            memory,
            startup_ms: startup.into_iter().map(|mark| (mark, 1.0)).collect(),
            redaction_misses: 0,
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn report_never_holds_canary_values(input in report_input()) {
        let dir = TempDir::new().unwrap();
        let logs = dir.path().join("logs");
        fs::create_dir(&logs).unwrap();
        let zip = dir.path().join("report.zip");
        write_report(&zip, &input, &redactor()).unwrap();
        let found = leaks(&haystacks(&zip, &logs));
        prop_assert!(found.is_empty(), "canary leaked: {:#?}", found);
    }
}
