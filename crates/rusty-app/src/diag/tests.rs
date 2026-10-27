use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

use serde_json::Value;

use std::sync::Arc;

use rusty_diag::EventRing;
use tracing_subscriber::Registry;
use tracing_subscriber::layer::SubscriberExt;

use super::*;
use crate::logging::{Diagnostics, Opened, log_config, open};

fn diagnostics(opened: Opened) -> Diagnostics {
    Diagnostics {
        redactor: Arc::new(redactor()),
        ring: Arc::new(EventRing::new(EventRing::DEFAULT_BUDGET)),
        misses: opened.misses(),
        caches: rusty_diag::CacheRegistry::default(),
    }
}

fn read_report(path: &Path) -> Value {
    let mut zip = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
    let mut text = String::new();
    zip.by_name("report.json")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

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
        ("COMPUTERNAME", "BENCH-PC"),
        ("USERDOMAIN", "CORP"),
        ("USERPROFILE", r"C:\Users\admin"),
    ]));
    assert_eq!(
        (id.user, id.computer, id.domain, id.profile),
        (
            Some("admin".to_owned()),
            Some("BENCH-PC".to_owned()),
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

#[cfg(not(windows))]
#[test]
fn dpi_awareness_is_unknown_without_windows() {
    assert_eq!(report_input().system.dpi_awareness, DpiAwareness::Unknown);
}

#[test]
fn the_report_holds_build_system_memory_and_timing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.zip");
    write(&path, Instant::now(), &diagnostics(Opened::default())).unwrap();
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

#[test]
fn the_report_carries_the_recent_events_and_the_miss_count() {
    let mut diagnostics = diagnostics(Opened::default());
    let opened = open(log_config(None), &diagnostics.redactor, &diagnostics.ring);
    diagnostics.misses = opened.misses();
    let subscriber = Registry::default().with(opened.layer);
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(path = "/home/admin/secret.txt", "copied");
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.zip");
    write(&path, Instant::now(), &diagnostics).unwrap();
    let report = read_report(&path);
    assert_eq!(report["recent_events"][0]["msg"], "copied");
    assert_eq!(report["redaction_misses"], 1);
}

fn cache_named<'a>(report: &'a Value, name: &str) -> &'a Value {
    report["memory"]["caches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|cache| cache["name"] == name)
        .unwrap_or_else(|| panic!("no cache {name} in {report:#}"))
}

#[test]
fn the_report_shows_the_listing_cache_against_its_budget() {
    let diagnostics = diagnostics(Opened::default());
    let cache = rusty_cache::ListingCache::default();
    let key = rusty_cache::CacheKey::new(rusty_core::Location::new("other://open").unwrap(), None);
    cache.pin(&key);
    cache.put(
        key,
        rusty_cache::CachedListing {
            rows: Arc::from([]),
            read_at: std::time::SystemTime::UNIX_EPOCH,
            folder_changed: None,
            complete: true,
        },
    );
    register_listings(&diagnostics.caches, &cache, None);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.zip");
    write(&path, Instant::now(), &diagnostics).unwrap();
    let report = read_report(&path);
    let listings = cache_named(&report, "listings in memory");
    assert_eq!(listings["budget"], 64 * 1024 * 1024);
    assert_eq!(listings["entries"], 1);
    assert_eq!(listings["pinned_bytes"], listings["bytes"]);
    assert_eq!(listings["unpinned_bytes"], 0);
}

#[test]
fn the_report_shows_the_event_ring_against_its_budget() {
    let diagnostics = diagnostics(Opened::default());
    register_events(&diagnostics.caches, &diagnostics.ring);
    diagnostics.ring.push("one event");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.zip");
    write(&path, Instant::now(), &diagnostics).unwrap();
    let events = cache_named(&read_report(&path), "diagnostic events").clone();
    assert_eq!(
        (
            events["bytes"].clone(),
            events["entries"].clone(),
            events["budget"].clone()
        ),
        (Value::from(9), Value::from(1), Value::from(2 * 1024 * 1024))
    );
}

#[test]
fn the_report_shows_cache_db_against_its_budget() {
    let diagnostics = diagnostics(Opened::default());
    let dir = tempfile::tempdir().unwrap();
    let cache = rusty_cache::ListingCache::default();
    let warm = rusty_cache::Warm {
        cache: cache.clone(),
        keys: rusty_cache::CacheKeys::default(),
        folders: Vec::new(),
        deadline: Instant::now() + std::time::Duration::from_secs(5),
    };
    let disk = rusty_cache::DiskListings::open(&dir.path().join("cache.db"), warm);
    register_listings(&diagnostics.caches, &cache, Some(&disk));
    let path = dir.path().join("report.zip");
    write(&path, Instant::now(), &diagnostics).unwrap();
    let stored = cache_named(&read_report(&path), "listings in cache.db").clone();
    assert_eq!(
        (stored["entries"].clone(), stored["budget"].clone()),
        (Value::from(0), Value::from(32 * 1024 * 1024))
    );
}

#[test]
fn the_on_demand_report_reads_cache_db_from_the_data_folder() {
    let diagnostics = diagnostics(Opened::default());
    let dir = tempfile::tempdir().unwrap();
    let keys = rusty_cache::CacheKeys::default();
    let folder = rusty_core::Location::new("other://open").unwrap();
    let cache = rusty_cache::ListingCache::default();
    cache.put(
        keys.key(&folder).unwrap(),
        rusty_cache::CachedListing {
            rows: Arc::from([]),
            read_at: std::time::SystemTime::UNIX_EPOCH,
            folder_changed: None,
            complete: true,
        },
    );
    let warm = rusty_cache::Warm {
        cache: rusty_cache::ListingCache::default(),
        keys: keys.clone(),
        folders: Vec::new(),
        deadline: Instant::now() + std::time::Duration::from_secs(5),
    };
    let disk = rusty_cache::DiskListings::open(&dir.path().join("cache.db"), warm);
    disk.save_folders(&cache, &keys, std::slice::from_ref(&folder));
    assert!(disk.close(std::time::Duration::from_secs(5)));
    let path = dir.path().join("report.zip");
    write_on_demand(&path, Instant::now(), &diagnostics, Some(dir.path())).unwrap();
    let stored = cache_named(&read_report(&path), "listings in cache.db").clone();
    assert_eq!(
        (stored["entries"].clone(), stored["budget"].clone()),
        (Value::from(1), Value::from(32 * 1024 * 1024))
    );
}
