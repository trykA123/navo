use super::*;
use crate::fixture;
use crate::privacy::{Report, assert_no_leaks};

#[test]
fn free_space_buckets_split_at_their_boundaries() {
    for (gib, bucket) in [
        (0, FreeSpace::Under10Gib),
        (10, FreeSpace::From10To50Gib),
        (49, FreeSpace::From10To50Gib),
        (50, FreeSpace::From50To200Gib),
        (200, FreeSpace::Over200Gib),
    ] {
        assert_eq!(FreeSpace::from_bytes(gib * GIB), bucket);
    }
    assert_eq!(FreeSpace::from_bytes(10 * GIB - 1), FreeSpace::Under10Gib);
}

#[test]
fn ram_rounds_up_to_whole_gib() {
    assert_eq!(ram_gib(16 * GIB), 16);
    assert_eq!(ram_gib(16 * GIB - (200 << 20)), 16);
    assert_eq!(ram_gib(0), 0);
}

#[test]
fn drive_letter_reads_plain_and_verbatim_paths_only() {
    assert_eq!(drive_letter(r"c:\bench"), Some('C'));
    assert_eq!(drive_letter(r"\\?\D:\bench"), Some('D'));
    assert_eq!(drive_letter(r"\\server\share"), None);
    assert_eq!(drive_letter("/tmp/x"), None);
    assert_eq!(drive_letter(""), None);
}

#[test]
fn summary_counts_cores_and_leaks_no_identity() {
    let fixture = fixture::set().unwrap();
    let summary = summarize(fixture.root());
    assert!(summary.cpu_cores >= 1);
    let json = serde_json::to_string(&Report::Machine(summary)).unwrap();
    let owner = fixture.root().to_string_lossy();
    assert_no_leaks(json.as_bytes(), &[&owner, fixture::PRIVATE_FOLDER]).unwrap();
    assert!(!json.contains('/') && !json.contains('\\'));
}

#[cfg(not(windows))]
#[test]
fn unreadable_facts_are_unknown_off_windows() {
    let summary = summarize(std::path::Path::new("/"));
    assert_eq!(summary.disk, DiskKind::Unknown);
    assert_eq!(summary.defender, DefenderState::Unknown);
    assert_eq!(summary.free_space, FreeSpace::Unknown);
    assert_eq!(summary.windows_build, None);
}
