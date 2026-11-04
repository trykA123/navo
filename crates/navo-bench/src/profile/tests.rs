use std::time::{Duration, SystemTime};

use super::histogram::Histogram;
use super::layouts as lay;
use super::*;
use crate::fixture;
use crate::privacy::{LeakScanner, Output, Report, SafeWriter, assert_no_leaks};
use crate::source::{EntryKind, test_open};

fn profile_at(now: SystemTime) -> Profile {
    let fx = fixture::set().unwrap();
    let source = test_open(fx.root()).unwrap();
    let (profile, summary) = collect(&source, WalkConfig::default(), now);
    assert_eq!(summary.errors, 0);
    profile
}

fn profile() -> Profile {
    profile_at(SystemTime::now())
}

fn bucket_files(rows: &[Report], group: &str, bucket: &str) -> u64 {
    rows.iter()
        .filter_map(|row| match serde_json::to_value(row).ok()? {
            value if value["group"] == group && value["bucket"] == bucket => {
                value["files"].as_u64()
            }
            _ => None,
        })
        .sum()
}

fn measurement(rows: &[Report], label: &str) -> Option<u64> {
    rows.iter().find_map(|row| {
        let value = serde_json::to_value(row).ok()?;
        (value["kind"] == "measurement" && value["label"] == label)
            .then(|| value["value"].as_u64())?
    })
}

#[test]
fn files_per_folder_percentiles_and_depth() {
    let p = profile();
    assert!(p.folder_files.max >= 12 || p.folder_files.max >= 6);
    assert!(p.folder_files.p50 <= p.folder_files.p95 && p.folder_files.p95 <= p.folder_files.max);
    assert!(p.max_depth >= 6);
    let rows = p.reports();
    assert!(bucket_files(&rows, "file-depth", "6-8") >= 1);
    assert!(bucket_files(&rows, "file-depth", "0") >= 1);
}

#[test]
fn percentile_uses_nearest_rank() {
    let values = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    assert_eq!(finish::percentile_for_tests(&values, 50), 5);
    assert_eq!(finish::percentile_for_tests(&values, 95), 10);
    assert_eq!(finish::percentile_for_tests(&[], 50), 0);
}

#[test]
fn size_buckets_and_big_file_thresholds() {
    let p = profile();
    assert_eq!(p.size.counts().iter().sum::<u64>(), p.counts.files);
    assert_eq!((p.counts.over_100_mb, p.counts.over_1_gb), (0, 0));
    let mut edge = Histogram::new(&lay::SIZE);
    for size in [
        0,
        4096,
        4097,
        100 * lay::MIB,
        100 * lay::MIB + 1,
        lay::GIB + 1,
    ] {
        edge.add(&lay::SIZE, size, size);
    }
    assert_eq!(edge.counts(), &[1, 1, 1, 0, 0, 1, 1, 1]);
}

#[test]
fn name_and_path_length_buckets() {
    let p = profile();
    assert!(p.counts.paths_over_260 >= 1);
    let rows = p.reports();
    assert!(bucket_files(&rows, "path-length", "over-260") >= 1);
    assert!(bucket_files(&rows, "name-length", "up-to-16") >= 1);
    assert_eq!(
        p.name_length.counts().iter().sum::<u64>(),
        p.counts.files
            + p.counts.folders
            + p.counts.symlinks
            + p.counts.junctions
            + p.counts.reparse_points
    );
}

#[test]
fn extensions_fold_rare_into_other() {
    let p = profile();
    let rows = p.reports();
    let json = serde_json::to_string(&rows).unwrap();
    let fold = p.extensions.iter().find(|mix| {
        serde_json::to_string(&mix.extension)
            .is_ok_and(|text| text == format!("\"{}\"", fixture::FOLD_EXT))
    });
    assert_eq!(fold.map(|mix| mix.files), Some(12));
    assert!(json.contains("\"other\""));
    assert!(!json.contains(fixture::RARE_EXT));
    assert!(!json.contains(fixture::PRIVATE_EXTENSION));
}

#[test]
fn kinds_come_from_extension_and_magic() {
    let c = profile().counts;
    assert_eq!(
        (c.archives, c.images, c.pdf, c.elf, c.map, c.a2l, c.dbc),
        (1, 1, 1, 1, 1, 1, 1)
    );
    assert!(c.text >= 3 && c.binary >= 2);
    assert_eq!(c.sniffed, c.text + c.binary);
}

#[test]
fn git_repositories_and_their_size() {
    let p = profile();
    assert_eq!(p.counts.git_repos, 1);
    assert_eq!(p.repo_size.counts().iter().sum::<u64>(), 1);
    assert_eq!(p.repo_size.counts().first().copied(), Some(1));
}

#[test]
fn links_and_attribute_counts() {
    let c = profile().counts;
    assert!(c.hard_link_checked >= 1);
    #[cfg(unix)]
    assert!(c.hard_linked >= 2);
    assert!(c.symlinks + c.junctions + c.reparse_points >= 1);
    assert!(c.hidden >= 1 && c.read_only >= 1);
}

#[test]
fn timestamp_spread_follows_the_clock() {
    let now = SystemTime::now();
    let p = profile_at(now);
    assert_eq!(p.age.counts().first().copied(), Some(p.counts.files));
    let later = now + Duration::from_secs(2 * 366 * 86_400);
    let p = profile_at(later);
    assert_eq!(p.age.counts().last().copied(), Some(p.counts.files));
}

#[test]
fn drive_and_file_system_rows_are_present() {
    let rows = profile().reports();
    let drive = rows
        .iter()
        .filter(|row| serde_json::to_string(row).is_ok_and(|text| text.contains("\"drive-")));
    assert_eq!(drive.count(), 1);
    assert_eq!(measurement(&rows, "files"), Some(profile().counts.files));
}

#[test]
fn the_serialised_profile_leaks_no_planted_name() {
    let rows = profile().reports();
    let planted = [
        fixture::MARKER,
        fixture::PRIVATE_FOLDER,
        fixture::PRIVATE_FILE,
        fixture::PRIVATE_EXTENSION,
        fixture::PRIVATE_QUERY,
        fixture::PRIVATE_CONTENT,
        fixture::KIND_DIR,
        fixture::KIND_TOOL,
        fixture::REPO_DIR,
        fixture::RARE_EXT,
    ];
    let json = serde_json::to_vec(&rows).unwrap();
    assert_no_leaks(&json, &planted).unwrap();
    let mut sink = Vec::new();
    let mut writer = SafeWriter::new(&mut sink, LeakScanner::new(&planted).unwrap());
    for row in &rows {
        writer.write(Output::Json, row).unwrap();
    }
    assert_no_leaks(&sink, &planted).unwrap();
    assert!(sink.len() > 100);
    let _ = EntryKind::File;
}
