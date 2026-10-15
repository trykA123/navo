use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

use serde_json::Value;

use super::*;

fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|&(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    move |name| map.get(name).cloned()
}

#[test]
fn identity_reads_the_windows_names() {
    let id = identity(env_of(&[
        ("USERNAME", "admin"),
        ("COMPUTERNAME", "WORK-PC"),
        ("USERDOMAIN", "CORP"),
        ("USERPROFILE", r"C:\Users\admin"),
    ]));
    assert_eq!(
        (id.user, id.computer, id.domain, id.profile),
        (
            Some("admin".to_owned()),
            Some("WORK-PC".to_owned()),
            Some("CORP".to_owned()),
            Some(r"C:\Users\admin".to_owned())
        )
    );
}

#[test]
fn identity_falls_back_to_the_unix_names() {
    let id = identity(env_of(&[
        ("USER", "admin"),
        ("HOSTNAME", "box"),
        ("HOME", "/home/admin"),
    ]));
    assert_eq!(
        (id.user, id.computer, id.domain, id.profile),
        (
            Some("admin".to_owned()),
            Some("box".to_owned()),
            None,
            Some("/home/admin".to_owned())
        )
    );
}

#[test]
fn empty_variables_count_as_missing() {
    let id = identity(env_of(&[("USERNAME", ""), ("USER", "admin")]));
    assert_eq!(id.user.as_deref(), Some("admin"));
}

#[test]
fn mem_total_is_read_in_bytes() {
    let text = "MemFree:  10 kB\nMemTotal:       16384 kB\n";
    assert_eq!(parse_mem_total(text), Some(16_777_216));
}

#[test]
fn malformed_mem_total_is_none() {
    assert_eq!(parse_mem_total("MemTotal: lots kB\n"), None);
}

#[test]
fn the_input_names_this_build() {
    let input = report_input();
    assert_eq!(input.build.version, env!("CARGO_PKG_VERSION"));
    assert!(!input.build.commit.is_empty());
}

#[test]
fn the_report_holds_build_system_memory_and_timing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.zip");
    write(&path, Instant::now()).unwrap();
    let mut zip = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();
    let mut text = String::new();
    zip.by_name("report.json")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    let report: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(report["build"]["version"], env!("CARGO_PKG_VERSION"));
    assert!(report["system"]["cpu_count"].as_u64() > Some(0));
    assert!(report["memory"].is_object());
    assert!(report["performance"].is_object());
}
