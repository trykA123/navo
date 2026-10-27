use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rusty_diag::components::ComponentUse;
use rusty_diag::heap::HeapStats;
use rusty_diag::perf::{
    FrameSample, GpuFacts, Latency, MemoryTrack, ProcessSample, Recording, SystemFacts, ThreadCpu,
    ThreadTime, UiState,
};
use rusty_diag::{CacheRegistry, EventRing, Identity, Redactor, Salt};
use rusty_ui::perf::PerfService;
use tracing_subscriber::Registry;
use tracing_subscriber::layer::SubscriberExt;

use super::{PerfReports, process, save_zip, system};
use crate::logging::{Diagnostics, log_config, open};

const FOLDER: &str = r"C:\Users\canary-user-7731\Documents\canary-plans-7731";
const QUERY: &str = "canary budget 7731";
const NEEDLES: [&str; 5] = ["canary", "7731", "salary", "plans", "documents"];

fn reports(folder: &Path) -> PerfReports {
    let redactor = Arc::new(Redactor::new(
        Salt::random(),
        Identity {
            user: Some("canary-user-7731".to_owned()),
            computer: Some("CANARY-PC-7731".to_owned()),
            domain: None,
            profile: Some(r"C:\Users\canary-user-7731".to_owned()),
        },
    ));
    let ring = Arc::new(EventRing::new(EventRing::DEFAULT_BUDGET));
    let opened = open(log_config(None), &redactor, &ring);
    let misses = opened.misses();
    let subscriber = Registry::default().with(opened.layer);
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(path = FOLDER, "opened folder");
        tracing::info!("listing {FOLDER}\\canary-salary-7731.xlsx took 4 ms");
    });
    PerfReports {
        started: Instant::now(),
        diagnostics: Diagnostics {
            redactor,
            ring,
            misses,
            caches: CacheRegistry::default(),
        },
        folder: Some(folder.to_owned()),
        data: Some(folder.join("data")),
        last_cpu: Mutex::new(None),
        threads: Mutex::new(seeded_threads()),
        memory: Mutex::new(seeded_memory()),
    }
}

fn seeded_memory() -> MemoryTrack {
    let mut track = MemoryTrack::default();
    let heap = HeapStats {
        live_bytes: 3 << 20,
        peak_bytes: 5 << 20,
        allocations: 40,
    };
    let walk = ComponentUse {
        name: "walk index",
        bytes: 2 << 20,
        items: 9,
        busy: false,
    };
    track.record(Instant::now(), Some(heap), vec![walk]);
    track
}

fn seeded_threads() -> ThreadCpu {
    let start = Instant::now();
    let mut threads = ThreadCpu::default();
    for (second, ms) in [(0, 0), (1, 700)] {
        let sample = ThreadTime {
            id: 7,
            name: Some(format!("{FOLDER} reader")),
            cpu: Duration::from_millis(ms),
        };
        threads.record(start + Duration::from_secs(second), &[sample]);
    }
    threads
}

fn seeded_recording() -> (Recording, SystemFacts) {
    let start = Instant::now();
    let mut recording = Recording::new(start);
    recording.frames.push(FrameSample {
        window: 0,
        at_ms: 1.0,
        interval_ms: Some(16.0),
        layout_ms: 4.0,
        paint_ms: 2.0,
        state: UiState::default(),
    });
    recording.push_latency(
        Latency::CommandCenterKey,
        start,
        start + Duration::from_millis(9),
    );
    recording.process.push(ProcessSample {
        cpu_percent: Some(12.0),
        ..ProcessSample::default()
    });
    let system = SystemFacts {
        os: format!("Windows 11 opened from {FOLDER}"),
        gpu: Some(GpuFacts {
            name: format!("GPU \"{QUERY}\" at {FOLDER}\\canary-salary-7731.xlsx"),
            ..GpuFacts::default()
        }),
        ..SystemFacts::default()
    };
    (recording, system)
}

fn entries(zip: &Path) -> Vec<(String, String)> {
    let mut archive = zip::ZipArchive::new(File::open(zip).unwrap()).unwrap();
    (0..archive.len())
        .map(|index| {
            let mut entry = archive.by_index(index).unwrap();
            let mut text = String::new();
            entry.read_to_string(&mut text).unwrap();
            (entry.name().unwrap().into_owned(), text)
        })
        .collect()
}

#[test]
fn the_zip_on_the_desktop_holds_the_diag_report_and_perf_json() {
    let dir = tempfile::tempdir().unwrap();
    let (recording, system) = seeded_recording();
    let zip = reports(dir.path())
        .save(&recording, Instant::now(), &system)
        .unwrap();
    assert_eq!(zip.parent(), Some(dir.path()));
    let name = zip.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name.starts_with("navo-performance-") && name.ends_with(".zip"),
        "{name}"
    );
    let names: Vec<String> = entries(&zip).into_iter().map(|(name, _)| name).collect();
    assert_eq!(
        names,
        [
            "report.json",
            "summary.txt",
            "perf.json",
            "perf-threads.txt",
            "perf-memory.txt"
        ]
    );
    let mut entries = entries(&zip);
    let (_, memory) = entries.pop().unwrap();
    assert!(
        memory.contains("Rust heap: 3.0 MiB live, 5.0 MiB peak"),
        "{memory}"
    );
    assert!(
        memory.contains("walk index: 2.0 MiB in 9 items"),
        "{memory}"
    );
    let (_, threads) = entries.pop().unwrap();
    assert!(
        threads.contains("(7): 700 ms total, peak 70.0 %"),
        "{threads}"
    );
    let (_, json) = entries.pop().unwrap();
    let json: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        json["samples"]["memory"][0]["components"]["walk index"],
        2 << 20
    );
}

#[test]
fn no_seeded_path_name_or_search_text_reaches_the_zip() {
    let dir = tempfile::tempdir().unwrap();
    let (recording, system) = seeded_recording();
    let zip = reports(dir.path())
        .save(&recording, Instant::now(), &system)
        .unwrap();
    let entries = entries(&zip);
    assert!(
        entries
            .iter()
            .any(|(_, text)| text.contains("command_center_key"))
    );
    for (name, text) in entries {
        let text = text.to_lowercase();
        for needle in NEEDLES {
            assert!(!text.contains(needle), "{needle} leaked into {name}");
        }
    }
}

#[test]
fn a_taken_name_gets_the_next_number() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("navo-performance-20261026-101500.zip"), b"").unwrap();
    let path = save_zip(dir.path(), "20261026-101500", |path| {
        if path.exists() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        fs::write(path, b"zip")
    })
    .unwrap();
    assert_eq!(
        path,
        dir.path().join("navo-performance-20261026-101500-2.zip")
    );
}

#[test]
fn a_failed_write_is_reported_and_leaves_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let saved = save_zip(dir.path(), "20261026-101500", |_| {
        Err(io::ErrorKind::PermissionDenied.into())
    });
    assert_eq!(saved.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn process_cpu_time_and_threads_come_from_proc_stat() {
    let stat = "1234 (fer rite) S 1 1234 1234 0 -1 4194560 100 0 0 0 250 50 0 0 20 0 17 0 12345 0";
    assert_eq!(process::cpu_from_stat(stat), Some(Duration::from_secs(3)));
    assert_eq!(process::threads_from_stat(stat), Some(17));
    assert_eq!(process::cpu_from_stat("garbage"), None);
}

#[test]
fn linux_names_come_from_os_release_and_cpuinfo() {
    let os = "NAME=\"CachyOS\"\nPRETTY_NAME=\"CachyOS Linux\"\nID=cachyos\n";
    assert_eq!(system::pretty_name(os).as_deref(), Some("CachyOS Linux"));
    let cpu = "processor\t: 0\nmodel name\t: AMD Ryzen 9 7950X 16-Core Processor\n";
    assert_eq!(
        system::model_name(cpu).as_deref(),
        Some("AMD Ryzen 9 7950X 16-Core Processor")
    );
}

#[test]
fn cpu_percent_needs_two_samples() {
    let dir = tempfile::tempdir().unwrap();
    let reports = reports(dir.path());
    let now = Instant::now();
    assert_eq!(reports.cpu_percent(now), None);
    assert!(
        reports
            .cpu_percent(now + Duration::from_secs(1))
            .is_some_and(|cpu| cpu >= 0.0)
    );
}

#[test]
fn a_new_recording_forgets_the_last_cpu_mark() {
    let dir = tempfile::tempdir().unwrap();
    let reports = reports(dir.path());
    let now = Instant::now();
    let _ = reports.cpu_percent(now);
    reports.start();
    assert_eq!(reports.cpu_percent(now + Duration::from_secs(600)), None);
}

#[test]
fn performance_zip_includes_disk_totals_and_redacted_profile_paths() {
    let root = tempfile::tempdir().unwrap();
    let reports = reports(root.path());
    let data = root.path().join("data");
    fs::create_dir_all(data.join("index")).unwrap();
    fs::write(data.join("cache.db-wal"), [0; 7]).unwrap();
    fs::write(data.join("navo.db-wal"), [0; 11]).unwrap();
    fs::write(data.join("index/walk.index"), [0; 13]).unwrap();
    let (recording, system) = seeded_recording();
    let zip = reports.save(&recording, Instant::now(), &system).unwrap();
    let diagnostic = root.path().join("diagnostic.zip");
    crate::diag::write_on_demand(
        &diagnostic,
        Instant::now(),
        &reports.diagnostics,
        Some(&data),
    )
    .unwrap();
    for zip in [zip, diagnostic] {
        let entries = entries(&zip);
        let report = entries
            .iter()
            .find(|(name, _)| name == "report.json")
            .unwrap();
        let json: serde_json::Value = serde_json::from_str(&report.1).unwrap();
        let locations = json["disk_use"]["locations"].as_array().unwrap();
        for (kind, bytes) in [("listing_cache", 7), ("journal", 11), ("index", 13)] {
            let total: u64 = locations
                .iter()
                .filter(|location| location["kind"] == kind)
                .map(|location| location["bytes"].as_u64().unwrap())
                .sum();
            assert_eq!(total, bytes);
        }
        let summary = entries
            .iter()
            .find(|(name, _)| name == "summary.txt")
            .unwrap();
        assert!(summary.1.contains("Disk use:"));
        for (_, text) in entries {
            assert!(!text.contains(data.to_str().unwrap()));
        }
    }
}
