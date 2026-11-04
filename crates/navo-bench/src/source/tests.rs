use super::*;
use crate::fixture;
use std::fs::{self, DirEntry, Metadata};

fn records(source: &ReadOnlySource) -> (Vec<Record>, WalkSummary) {
    let mut entries = Vec::new();
    let summary = source.walk(WalkConfig::default(), |record| entries.push(record));
    (entries, summary)
}

/// `path` in the form records carry: below the canonical folder, which is `\\?\` on Windows.
fn in_source(source: &ReadOnlySource, fx: &fixture::Fixture, path: &Path) -> PathBuf {
    source.folder().join(path.strip_prefix(fx.root()).unwrap())
}

#[test]
fn fixture_walk_records_metadata_without_entering_external_links() {
    let fx = fixture::set().unwrap();
    let link = fx
        .link_out
        .as_ref()
        .expect("the planted external link must exist");
    let source = crate::source::test_open(fx.root()).unwrap();
    let link = &in_source(&source, &fx, link);
    let outside = fs::canonicalize(fx.outside.path()).unwrap();
    let (entries, summary) = records(&source);
    assert_eq!(summary.outcome, WalkOutcome::Complete);
    assert_eq!(summary.errors, 0);
    assert_eq!(summary.files, u64::try_from(entries.len()).unwrap());
    let external_link = entries
        .iter()
        .find(|record| *record.id.path == *link)
        .unwrap();
    assert!(matches!(
        external_link.kind,
        EntryKind::SymbolicLink | EntryKind::Junction | EntryKind::ReparsePoint
    ));
    assert!(
        !entries
            .iter()
            .any(|record| record.id.path.starts_with(link) && record.id.path.as_path() != link)
    );
    assert!(
        !entries
            .iter()
            .any(|record| record.id.path.starts_with(&outside))
    );
    assert!(!entries.iter().any(|record| {
        record
            .id
            .path
            .file_name()
            .is_some_and(|name| name == "outside.txt")
    }));
    assert_eq!(
        source.open_read(&external_link.id).unwrap_err(),
        SourceError::Link
    );
}

#[test]
fn fixture_records_include_metadata_and_relative_depth() {
    let fx = fixture::set().unwrap();
    let source = crate::source::test_open(fx.root()).unwrap();
    let (entries, _) = records(&source);
    let marker = entries
        .iter()
        .find(|record| *record.id.path == in_source(&source, &fx, &fx.marker_file))
        .unwrap();
    assert_eq!(marker.kind, EntryKind::File);
    assert_eq!(marker.depth, 0);
    assert_eq!(marker.size, u64::try_from(fixture::CONTENT.len()).unwrap());
    assert_eq!(marker.extension, "txt");
    assert!(marker.mtime.is_some());
    #[cfg(unix)]
    assert_eq!(marker.hard_links, Some(2));
    let deep = entries
        .iter()
        .find(|record| *record.id.path == in_source(&source, &fx, &fx.long_file))
        .unwrap();
    assert!(deep.depth > 0);
    assert!(deep.parent.is_some());
}

#[test]
fn content_reads_require_an_id_from_this_source() {
    let fx = fixture::set().unwrap();
    let source = crate::source::test_open(fx.root()).unwrap();
    let (entries, _) = records(&source);
    let record = entries
        .iter()
        .find(|record| *record.id.path == in_source(&source, &fx, &fx.marker_file))
        .unwrap();
    let mut file = source.open_read(&record.id).unwrap();
    let mut content = Vec::new();
    file.read_to_end(&mut content).unwrap();
    assert_eq!(content, fixture::CONTENT);
    let other = crate::source::test_open(fx.root()).unwrap();
    assert_eq!(
        other.open_read(&record.id).unwrap_err(),
        SourceError::ForeignId
    );
    let directory = entries
        .iter()
        .find(|record| record.kind == EntryKind::Directory)
        .unwrap();
    assert_eq!(
        source.open_read(&directory.id).unwrap_err(),
        SourceError::NotFile
    );
}

#[test]
fn count_budget_stops_at_the_limit_and_does_not_descend_after_it() {
    let fx = fixture::set().unwrap();
    let source = crate::source::test_open(fx.root()).unwrap();
    let (_, full) = records(&source);
    for cap in [0, 1, full.files - 1, full.files, full.files + 1] {
        let mut seen = 0;
        let summary = source.walk(
            WalkConfig {
                max_files: cap,
                ..WalkConfig::default()
            },
            |_| seen += 1,
        );
        assert_eq!(summary.files, cap.min(full.files));
        assert_eq!(seen, summary.files);
        if cap <= full.files {
            assert_eq!(
                summary.outcome,
                WalkOutcome::StoppedAt {
                    files: cap,
                    reason: StopReason::FileCount
                }
            );
            assert_eq!(summary.outcome.to_string(), format!("stopped at {cap}"));
        } else {
            assert_eq!(summary.outcome, WalkOutcome::Complete);
        }
    }
}

struct FakeIo {
    now: Duration,
    step: Duration,
    fail_marker: bool,
}

impl walk::WalkIo for FakeIo {
    fn elapsed(&mut self) -> Duration {
        let now = self.now;
        self.now += self.step;
        now
    }

    fn metadata(&mut self, entry: &DirEntry) -> io::Result<Metadata> {
        if self.fail_marker
            && entry
                .file_name()
                .to_string_lossy()
                .contains(fixture::MARKER)
        {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "secret-path-must-not-escape",
            ))
        } else {
            entry.metadata()
        }
    }
}

#[test]
fn fake_clock_stops_before_at_and_after_the_deadline() {
    let fx = fixture::set().unwrap();
    let source = crate::source::test_open(fx.root()).unwrap();
    let limit = Duration::from_secs(60);
    for now in [
        limit - Duration::from_nanos(1),
        limit,
        limit + Duration::from_nanos(1),
    ] {
        let mut clock = FakeIo {
            now,
            step: Duration::ZERO,
            fail_marker: false,
        };
        let summary = walk::run_with(&source, WalkConfig::default(), |_| true, &mut clock);
        if now < limit {
            assert_eq!(summary.outcome, WalkOutcome::Complete);
        } else {
            assert_eq!(summary.files, 0);
            assert_eq!(
                summary.outcome,
                WalkOutcome::StoppedAt {
                    files: 0,
                    reason: StopReason::Time
                }
            );
        }
    }
}

#[test]
fn a_running_walk_stops_when_the_fake_clock_reaches_the_deadline() {
    let fx = fixture::set().unwrap();
    let source = crate::source::test_open(fx.root()).unwrap();
    let mut clock = FakeIo {
        now: Duration::ZERO,
        step: Duration::from_secs(1),
        fail_marker: false,
    };
    let summary = walk::run_with(
        &source,
        WalkConfig {
            max_time: Duration::from_secs(3),
            ..WalkConfig::default()
        },
        |_| true,
        &mut clock,
    );
    assert!(summary.files > 0);
    assert_eq!(
        summary.outcome,
        WalkOutcome::StoppedAt {
            files: summary.files,
            reason: StopReason::Time
        }
    );
}

#[test]
fn unreadable_metadata_is_counted_and_other_entries_continue() {
    let fx = fixture::set().unwrap();
    let source = crate::source::test_open(fx.root()).unwrap();
    let (_, complete) = records(&source);
    let mut fault = FakeIo {
        now: Duration::ZERO,
        step: Duration::ZERO,
        fail_marker: true,
    };
    let mut count = 0;
    let visit = |_| {
        count += 1;
        true
    };
    let summary = walk::run_with(&source, WalkConfig::default(), visit, &mut fault);
    assert_eq!(summary.outcome, WalkOutcome::Complete);
    assert_eq!(summary.errors, 1);
    assert_eq!(summary.files, complete.files);
    assert_eq!(count + 1, summary.files);
    assert!(!format!("{summary:?}").contains("secret-path"));
}

#[cfg(unix)]
#[test]
fn an_unreadable_directory_counts_an_error_without_aborting() {
    let fx = fixture::set().unwrap();
    let source = crate::source::test_open(fx.root()).unwrap();
    let (_, complete) = records(&source);
    let summary = fx
        .with_unreadable_directory(|| source.walk(WalkConfig::default(), |_| {}))
        .unwrap();
    assert_eq!(summary.errors, 1);
    assert_eq!(summary.files + 1, complete.files);
    assert_eq!(summary.outcome, WalkOutcome::Complete);
}

#[test]
fn debug_and_display_never_reveal_planted_names_or_paths() {
    let fx = fixture::set().unwrap();
    let source = crate::source::test_open(fx.root()).unwrap();
    let (entries, _) = records(&source);
    let mut output = format!("{source:?}{entries:?}");
    for entry in &entries {
        output.push_str(&format!("{:?}", entry.id));
        let mut hasher = RecordingHasher::default();
        std::hash::Hash::hash(&entry.id, &mut hasher);
        assert!(
            !hasher
                .0
                .windows(fixture::MARKER.len())
                .any(|bytes| bytes == fixture::MARKER.as_bytes())
        );
        if entry.kind == EntryKind::File {
            output.push_str(&format!("{:?}", source.open_read(&entry.id).unwrap()));
        }
    }
    let error = crate::source::test_open(&fx.root().join("secret-missing")).unwrap_err();
    output.push_str(&format!("{error}{error:?}"));
    assert!(!output.contains(fixture::MARKER));
    assert!(!output.contains("secret-missing"));
    assert!(!output.contains(&fx.root().to_string_lossy().to_string()));
}

#[derive(Default)]
struct RecordingHasher(Vec<u8>);

impl std::hash::Hasher for RecordingHasher {
    fn finish(&self) -> u64 {
        0
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
}

#[test]
fn a_file_or_a_link_cannot_be_selected_as_the_source() {
    let fx = fixture::set().unwrap();
    assert_eq!(
        crate::source::test_open(&fx.marker_file).unwrap_err(),
        SourceError::NotDirectory
    );
    assert_eq!(
        crate::source::test_open(fx.link_out.as_ref().unwrap()).unwrap_err(),
        SourceError::Link
    );
}

#[cfg(target_os = "linux")]
#[test]
fn retained_and_walking_source_handles_are_all_read_only() {
    let fx = fixture::set().unwrap();
    let source = crate::source::test_open(fx.root()).unwrap();
    let (entries, _) = records(&source);
    let record = entries
        .iter()
        .find(|record| record.kind == EntryKind::File)
        .unwrap();
    let _reader = source.open_read(&record.id).unwrap();
    let check = || {
        let mut count = 0;
        for entry in fs::read_dir("/proc/self/fd").unwrap() {
            let entry = entry.unwrap();
            let Ok(target) = fs::read_link(entry.path()) else {
                continue;
            };
            if !target.starts_with(fx.root()) {
                continue;
            }
            // The walk worker may close a handle between the two reads.
            let Ok(info) =
                fs::read_to_string(Path::new("/proc/self/fdinfo").join(entry.file_name()))
            else {
                continue;
            };
            let flags = info
                .lines()
                .find_map(|line| line.strip_prefix("flags:\t"))
                .unwrap();
            let flags = u32::from_str_radix(flags, 8).unwrap();
            assert_eq!(
                flags & u32::try_from(libc::O_ACCMODE).unwrap(),
                u32::try_from(libc::O_RDONLY).unwrap()
            );
            count += 1;
        }
        assert!(
            count >= 2,
            "source and content handles must remain open during inspection"
        );
    };
    check();
    source.walk(WalkConfig::default(), |_| check());
}

#[cfg(windows)]
#[path = "windows_tests.rs"]
mod windows;
