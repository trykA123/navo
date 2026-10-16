use std::path::PathBuf;

use rusty_diag::{Identity, Salt};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::Registry;
use tracing_subscriber::layer::SubscriberExt;

use super::*;

fn redactor() -> Arc<Redactor> {
    Arc::new(Redactor::new(
        Salt::from_bytes([7; 16]),
        Identity {
            user: Some("admin".to_owned()),
            ..Identity::default()
        },
    ))
}

fn ring() -> Arc<EventRing> {
    Arc::new(EventRing::new(EventRing::DEFAULT_BUDGET))
}

fn emit_into(opened: Opened, emit: impl FnOnce()) {
    tracing::subscriber::with_default(Registry::default().with(opened.layer), emit);
}

#[test]
fn logs_live_in_a_logs_folder_under_the_data_root() {
    let config = log_config(Some(Path::new("/data/ferrite")));
    assert_eq!(config.dir, Some(PathBuf::from("/data/ferrite/logs")));
}

#[test]
fn log_files_roll_at_five_mebibytes_and_keep_five() {
    let config = log_config(Some(Path::new("/data/ferrite")));
    assert_eq!(
        (
            config.file_stem.as_str(),
            config.max_file_bytes,
            config.files
        ),
        ("ferrite", 5 * 1024 * 1024, 5)
    );
}

#[test]
fn no_data_root_means_ring_only() {
    assert_eq!(log_config(None).dir, None);
}

#[test]
fn a_writable_folder_receives_the_log_file() {
    let root = tempfile::tempdir().unwrap();
    let ring = ring();
    let opened = open(log_config(Some(root.path())), &redactor(), &ring);
    assert!(opened.file_error.is_none());
    emit_into(opened, || tracing::info!("hello"));
    let text = std::fs::read_to_string(root.path().join("logs/ferrite.log")).unwrap();
    assert!(text.contains("hello"));
}

#[test]
fn an_unwritable_folder_falls_back_to_the_ring() {
    let root = tempfile::tempdir().unwrap();
    let blocker = root.path().join("blocker");
    std::fs::write(&blocker, b"").unwrap();
    let ring = ring();
    let opened = open(log_config(Some(&blocker)), &redactor(), &ring);
    assert!(opened.file_error.is_some());
    emit_into(opened, || tracing::info!("still recorded"));
    assert!(
        ring.snapshot()
            .iter()
            .any(|line| line.contains("still recorded"))
    );
}

#[test]
fn the_default_level_is_info() {
    let (filter, problem) = filter(None);
    assert_eq!(
        (filter.max_level_hint(), problem),
        (Some(LevelFilter::INFO), None)
    );
}

#[test]
fn an_empty_variable_keeps_the_default() {
    assert_eq!(
        filter(Some("  ")).0.max_level_hint(),
        Some(LevelFilter::INFO)
    );
}

#[test]
fn ferrite_log_overrides_the_level() {
    let (filter, problem) = filter(Some("debug"));
    assert_eq!(
        (filter.max_level_hint(), problem),
        (Some(LevelFilter::DEBUG), None)
    );
}

#[test]
fn ferrite_log_accepts_target_directives() {
    let (filter, problem) = filter(Some("info,rusty_app=trace"));
    assert_eq!(
        (filter.max_level_hint(), problem),
        (Some(LevelFilter::TRACE), None)
    );
}

#[test]
fn a_bad_directive_keeps_info_and_says_why() {
    let (filter, problem) = filter(Some("rusty_app=loud"));
    assert_eq!(filter.max_level_hint(), Some(LevelFilter::INFO));
    assert!(problem.is_some_and(|text| text.contains(ENV_VAR)));
}

#[test]
fn the_start_event_names_the_version_and_commit() {
    let ring = ring();
    emit_into(open(log_config(None), &redactor(), &ring), log_start);
    let lines = ring.snapshot();
    let line: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(
        (
            &line["msg"],
            &line["fields"]["version"],
            line["fields"]["commit"].is_string()
        ),
        (
            &"app started".into(),
            &env!("CARGO_PKG_VERSION").into(),
            true
        )
    );
}
