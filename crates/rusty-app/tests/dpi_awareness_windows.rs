#![cfg(windows)]

use std::fs::File;
use std::io::Read;
use std::process::Command;

#[test]
fn ferrite_report_has_per_monitor_v2_awareness() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.zip");
    let output = Command::new(env!("CARGO_BIN_EXE_ferrite"))
        .arg("--diag-report")
        .arg(&path)
        .env("APPDATA", dir.path().join("config"))
        .env("LOCALAPPDATA", dir.path().join("data"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");

    let mut zip = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
    let mut text = String::new();
    zip.by_name("report.json")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    let report: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(report["system"]["dpi_awareness"], "per-monitor-v2");
}
