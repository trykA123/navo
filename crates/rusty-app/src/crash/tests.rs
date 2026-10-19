use std::cell::Cell;
use std::fs;
use std::io::Read;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rusty_diag::{EventRing, Identity, Redactor, Salt};
use serde_json::Value;
use tracing_subscriber::Registry;
use tracing_subscriber::layer::SubscriberExt;

use super::*;
use crate::logging::{self, Diagnostics, Opened};

static HOOK_LOCK: Mutex<()> = Mutex::new(());
thread_local! {
    // Other tests in this binary panic in parallel; only this test's threads reach the crash hook.
    static OWNED: Cell<bool> = const { Cell::new(false) };
}
const MESSAGE: &str = r"user=canary-user-7731 host=CANARY-PC-7731 domain=CANARYDOM C:\Users\canary-user-7731\Documents\canary-project-7731 \\canary-srv-7731\share\canary-7731\q3.xlsx cannot rename 'Secret Plan.docx'";

fn diagnostics() -> Diagnostics {
    Diagnostics {
        redactor: Arc::new(Redactor::new(
            Salt::from_bytes([7; 16]),
            Identity {
                user: Some("canary-user-7731".into()),
                computer: Some("CANARY-PC-7731".into()),
                domain: Some("CANARYDOM".into()),
                profile: Some(r"C:\Users\canary-user-7731".into()),
            },
        )),
        ring: Arc::new(EventRing::new(EventRing::DEFAULT_BUDGET)),
        misses: Opened::default().misses(),
        caches: rusty_diag::CacheRegistry::default(),
    }
}

fn with_hook(data: &Path, diagnostics: &Diagnostics, run: impl FnOnce(Arc<AtomicUsize>)) {
    let _lock = HOOK_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let previous = Arc::new(panic::take_hook());
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    panic::set_hook(Box::new(move |_| {
        counter.fetch_add(1, Ordering::Relaxed);
    }));
    install(Some(data), diagnostics);
    let installed = panic::take_hook();
    let elsewhere = previous.clone();
    panic::set_hook(Box::new(move |info| {
        if OWNED.get() {
            installed(info);
        } else {
            elsewhere(info);
        }
    }));
    own_thread();
    let result = panic::catch_unwind(AssertUnwindSafe(|| run(calls)));
    OWNED.set(false);
    drop(panic::take_hook());
    panic::set_hook(
        Arc::try_unwrap(previous).unwrap_or_else(|shared| Box::new(move |info| shared(info))),
    );
    if let Err(payload) = result {
        panic::resume_unwind(payload);
    }
}

fn own_thread() {
    OWNED.set(true);
}

fn trigger(message: &str) {
    assert!(panic::catch_unwind(|| panic!("{message}")).is_err());
}

fn crash_files(data: &Path) -> Vec<std::path::PathBuf> {
    let mut files: Vec<_> = fs::read_dir(data.join("logs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    files
}

fn assert_no_canaries_in_zip(zip: &mut zip::ZipArchive<fs::File>) {
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).unwrap();
        let mut text = String::new();
        entry.read_to_string(&mut text).unwrap();
        for needle in ["canary", "7731", "canarydom", "secret plan"] {
            assert!(
                !text.to_lowercase().contains(needle),
                "leaked {needle} in {:?}: {text}",
                entry.name()
            );
        }
    }
}

#[test]
fn hook_redacts_message_thread_and_events() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("logs")).unwrap();
    let diagnostics = diagnostics();
    let opened = logging::open(
        logging::log_config(None),
        &diagnostics.redactor,
        &diagnostics.ring,
    );
    tracing::subscriber::with_default(Registry::default().with(opened.layer), || {
        for index in 0..205 {
            tracing::info!(index, "{MESSAGE}");
        }
    });
    with_hook(dir.path(), &diagnostics, |_| {
        let thread = std::thread::Builder::new()
            .name("canary-user-7731".into())
            .spawn(|| {
                own_thread();
                panic!("{MESSAGE}")
            })
            .unwrap();
        assert!(thread.join().is_err());
    });
    let files = crash_files(dir.path());
    assert_eq!(files.len(), 1);
    let text = fs::read_to_string(&files[0]).unwrap();
    for needle in [
        "canary",
        "7731",
        "canarydom",
        "secret plan",
        "secret plan.docx",
    ] {
        assert!(
            !text.to_lowercase().contains(needle),
            "leaked {needle}: {text}"
        );
    }
    let record: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(record["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(record["commit"], env!("FERRITE_COMMIT"));
    assert_eq!(record["thread"], "<user>");
    assert!(record["location"].as_str().unwrap().contains("tests.rs:"));
    assert!(!record["backtrace"].as_str().unwrap().is_empty());
    let events = record["events"].as_array().unwrap();
    assert_eq!(events.len(), 200);
    let first: Value = serde_json::from_str(events[0].as_str().unwrap()).unwrap();
    let last: Value = serde_json::from_str(events[199].as_str().unwrap()).unwrap();
    assert_eq!(first["fields"]["index"], 5);
    assert_eq!(last["fields"]["index"], 204);
}

#[test]
fn hook_chains_to_the_previous_hook() {
    let dir = tempfile::tempdir().unwrap();
    with_hook(dir.path(), &diagnostics(), |calls| {
        trigger("test panic");
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    });
}

#[test]
fn hook_keeps_the_five_newest_crash_files() {
    let dir = tempfile::tempdir().unwrap();
    let logs = dir.path().join("logs");
    fs::create_dir(&logs).unwrap();
    for second in 0..7 {
        fs::write(
            logs.join(format!("crash-20000101-00000{second}-1.json")),
            "{}",
        )
        .unwrap();
    }
    fs::write(logs.join("ferrite.log"), "keep").unwrap();
    fs::write(logs.join("crash-not-a-record.json"), "keep").unwrap();
    with_hook(dir.path(), &diagnostics(), |_| trigger("test panic"));
    assert_eq!(crash_files(dir.path()).len(), 6);
    assert!(!logs.join("crash-20000101-000002-1.json").exists());
    assert!(logs.join("crash-20000101-000003-1.json").exists());
    assert_eq!(
        fs::read_to_string(logs.join("ferrite.log")).unwrap(),
        "keep"
    );
    assert_eq!(
        fs::read_to_string(logs.join("crash-not-a-record.json")).unwrap(),
        "keep"
    );
}

#[test]
fn hook_never_panics_when_the_logs_folder_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    with_hook(dir.path(), &diagnostics(), |calls| {
        trigger("test panic");
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    });
}

#[cfg(unix)]
#[test]
fn hook_never_panics_when_the_logs_folder_is_read_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let logs = dir.path().join("logs");
    fs::create_dir(&logs).unwrap();
    fs::set_permissions(&logs, fs::Permissions::from_mode(0o500)).unwrap();
    with_hook(dir.path(), &diagnostics(), |calls| {
        trigger("test panic");
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    });
    fs::set_permissions(&logs, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(crash_files(dir.path()).is_empty());
}

#[test]
fn hook_never_panics_when_the_logs_path_is_a_file() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("logs"), "blocker").unwrap();
    with_hook(dir.path(), &diagnostics(), |calls| {
        trigger("test panic");
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    });
}

#[test]
fn running_marker_starts_after_the_instance_lock_and_finishes_on_clean_quit() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    assert!(!data.join("running").exists());
    let lock = crate::instance::acquire_in(&data, dir.path()).unwrap();
    assert!(matches!(lock, crate::instance::Acquired::First(_)));
    assert!(!data.join("running").exists());
    let running = Running::start(&data).unwrap();
    assert!(data.join("running").is_file());
    running.finish().unwrap();
    assert!(!data.join("running").exists());
    assert!(pending(&data).is_none());
}

#[test]
fn running_marker_survives_a_simulated_crash() {
    let dir = tempfile::tempdir().unwrap();
    {
        let _running = Running::start(dir.path()).unwrap();
    }
    assert!(dir.path().join("running").exists());
}

#[test]
fn pending_finds_an_unclean_exit_without_crash_files() {
    let dir = tempfile::tempdir().unwrap();
    let _running = Running::start(dir.path()).unwrap();
    assert!(pending(dir.path()).unwrap().files.is_empty());
}

#[test]
fn pending_lists_crash_records_only_after_an_unclean_exit() {
    let dir = tempfile::tempdir().unwrap();
    let logs = dir.path().join("logs");
    fs::create_dir(&logs).unwrap();
    let file = logs.join("crash-20261019-120000-1.json");
    fs::write(&file, "{}").unwrap();
    fs::write(logs.join("ferrite.log"), "log").unwrap();
    assert!(pending(dir.path()).is_none());
    let running = Running::start(dir.path()).unwrap();
    assert_eq!(pending(dir.path()).unwrap().files, [file]);
    running.finish().unwrap();
    assert!(pending(dir.path()).is_none());
}

#[test]
fn clear_acknowledges_the_previous_marker_once() {
    let dir = tempfile::tempdir().unwrap();
    let _running = Running::start(dir.path()).unwrap();
    let previous = pending(dir.path()).unwrap();
    clear(&previous).unwrap();
    clear(&previous).unwrap();
    assert!(pending(dir.path()).is_none());
}

#[test]
fn clear_preserves_the_current_run_marker_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let _previous_run = Running::start(dir.path()).unwrap();
    let previous = pending(dir.path()).unwrap();
    let _current_run = Running::start(dir.path()).unwrap();
    clear(&previous).unwrap();
    assert!(dir.path().join("running").exists());
    assert!(pending(dir.path()).is_some());
}

#[test]
fn save_report_contains_the_normal_report_and_all_crash_files() {
    let dir = tempfile::tempdir().unwrap();
    let downloads = tempfile::tempdir().unwrap();
    let logs = dir.path().join("logs");
    fs::create_dir(&logs).unwrap();
    let diagnostics = diagnostics();
    with_hook(dir.path(), &diagnostics, |_| trigger(MESSAGE));
    for second in 0..2 {
        fs::write(
            logs.join(format!("crash-20261019-12000{second}-1.json")),
            "{\"message\":\"test panic\"}",
        )
        .unwrap();
    }
    let _running = Running::start(dir.path()).unwrap();
    let previous = pending(dir.path()).unwrap();
    let path = save_report(&previous, Instant::now(), &diagnostics, downloads.path()).unwrap();
    assert_eq!(path.parent(), Some(downloads.path()));
    let name = path.file_name().unwrap().to_str().unwrap();
    assert!(name.starts_with("ferrite-report-") && name.ends_with(".zip"));
    assert_eq!(name.len(), "ferrite-report-20261019-1200.zip".len());
    let mut zip = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
    assert_eq!(zip.len(), 5);
    assert!(zip.by_name("summary.txt").is_ok());
    let report: Value = serde_json::from_reader(zip.by_name("report.json").unwrap()).unwrap();
    assert!(report["memory"].is_object());
    assert!(report["performance"].is_object());
    for file in &previous.files {
        let mut text = String::new();
        zip.by_name(&format!(
            "crashes/{}",
            file.file_name().unwrap().to_str().unwrap()
        ))
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
        assert_eq!(text, fs::read_to_string(file).unwrap());
    }
    assert_no_canaries_in_zip(&mut zip);
    assert!(pending(dir.path()).is_some());
    let again = save_report(&previous, Instant::now(), &diagnostics, downloads.path()).unwrap();
    assert_ne!(again, path);
    assert!(path.exists() && again.exists());
}

#[test]
fn save_report_skips_a_crash_file_deleted_after_pending() {
    let dir = tempfile::tempdir().unwrap();
    let downloads = tempfile::tempdir().unwrap();
    let logs = dir.path().join("logs");
    fs::create_dir(&logs).unwrap();
    let removed = logs.join("crash-20261019-120000-1.json");
    let remaining = logs.join("crash-20261019-120001-1.json");
    for file in [&removed, &remaining] {
        fs::write(file, "{\"message\":\"test panic\"}").unwrap();
    }
    let _running = Running::start(dir.path()).unwrap();
    let previous = pending(dir.path()).unwrap();
    assert_eq!(previous.files, [removed.clone(), remaining.clone()]);
    fs::remove_file(&removed).unwrap();
    let path = save_report(&previous, Instant::now(), &diagnostics(), downloads.path()).unwrap();
    let mut zip = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    assert_eq!(zip.len(), 3);
    assert!(zip.by_name("report.json").is_ok());
    assert!(zip.by_name("summary.txt").is_ok());
    let mut text = String::new();
    zip.by_name("crashes/crash-20261019-120001-1.json")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert_eq!(text, fs::read_to_string(remaining).unwrap());
}

#[test]
fn two_panics_in_the_same_second_keep_both_records() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("logs")).unwrap();
    with_hook(dir.path(), &diagnostics(), |_| {
        trigger("first");
        trigger("second");
    });
    let files = crash_files(dir.path());
    assert_eq!(files.len(), 2, "{files:?}");
    assert_eq!(hook::files(&dir.path().join("logs")).len(), 2);
}

#[test]
fn locations_keep_only_the_crate_relative_part() {
    assert_eq!(
        hook::short_location(
            r"C:\Users\builder\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\gpui-kit-0.7.3\src\app.rs"
        ),
        "gpui-kit-0.7.3/src/app.rs"
    );
    assert_eq!(
        hook::short_location(
            "/home/builder/.cargo/git/checkouts/zed-0123abcd/4f5e6a7/crates/gpui/src/window.rs"
        ),
        "4f5e6a7/crates/gpui/src/window.rs"
    );
    assert_eq!(
        hook::short_location(r"crates\rusty-ui\src\pane.rs"),
        "crates/rusty-ui/src/pane.rs"
    );
    assert_eq!(
        hook::short_location("/home/builder/elsewhere/lib.rs"),
        "lib.rs"
    );
}
