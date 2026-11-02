use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::{Capture, TARGET, capture_with, config, redacted, redactor};

const MESSAGES: [&str; 7] = [
    r"session start user=canary-user-7731 host=CANARY-PC-7731 domain=CANARYDOM",
    r"open folder C:\Users\canary-user-7731\Documents\canary-project-7731",
    r"listing \\canary-srv-7731\share\Reports\2026 took 412 ms (network)",
    r"lock owner: CANARYDOM\canary-user-7731 (pid 4412)",
    r"watcher started on c:/users/canary-user-7731/.cargo/registry",
    r"shutdown: CANARY-PC-7731 idle, canary-user-7731 logged off",
    r"cannot rename 'Secret Plan.docx'",
];

const RAW_PATHS: [&str; 3] = [
    r"C:\Users\canary-user-7731\Desktop\canary-secret-7731.txt",
    r"\\canary-srv-7731\share\canary-7731.zip",
    r"\\?\UNC\canary-srv-7731\share\canary-7731\q3.xlsx",
];

fn emit_session(round: usize) {
    let r = redactor();
    let msg = MESSAGES[round % MESSAGES.len()];
    let raw = RAW_PATHS[round % RAW_PATHS.len()];
    let safe = redacted(&r, raw);
    match round % 5 {
        0 => tracing::trace!(target: TARGET, user = "canary-user-7731", path = raw, "{msg}"),
        1 => tracing::debug!(target: TARGET, host = %"CANARY-PC-7731", path = %safe, "{msg}"),
        2 => tracing::info!(target: TARGET, detail = raw, source_path = %raw, "{msg}"),
        3 => {
            tracing::warn!(target: TARGET, owner = ?"CANARYDOM\\canary-user-7731", dest_path = raw, "{msg}");
        }
        _ => tracing::error!(target: TARGET, domain = "CANARYDOM", path = %safe, code = 7, "{msg}"),
    }
}

fn session(dir: &Path) -> Capture {
    capture_with(config(Some(dir), 10 * 1024 * 1024), redactor(), || {
        (0..30).for_each(emit_session);
    })
}

fn leaks(text: &str) -> Vec<String> {
    text.lines()
        .filter(|l| {
            let lower = l.to_lowercase();
            lower.contains("canary") || lower.contains("7731") || lower.contains("secret plan.docx")
        })
        .map(str::to_owned)
        .collect()
}

#[test]
fn canary_session_logs_thirty_events() {
    let dir = TempDir::new().unwrap();
    assert_eq!(session(dir.path()).raw_lines().len(), 30);
}

#[test]
fn canary_values_never_reach_the_ring() {
    let dir = TempDir::new().unwrap();
    let ring = session(dir.path()).raw_lines().join("\n");
    let found = leaks(&ring);
    assert!(found.is_empty(), "canary leaked: {found:#?}");
}

#[test]
fn canary_values_never_reach_the_files() {
    let dir = TempDir::new().unwrap();
    session(dir.path());
    let text: String = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| fs::read_to_string(e.unwrap().path()).unwrap())
        .collect();
    let found = leaks(&text);
    assert!(found.is_empty(), "canary leaked: {found:#?}");
}
