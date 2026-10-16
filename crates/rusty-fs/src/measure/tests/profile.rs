use std::ops::ControlFlow;
use std::time::{Duration, Instant};

use rusty_core::{FsErrorKind, ProviderPath};

use crate::{
    Attributes, CancelToken, EntryKind, FakeFs, FakeOp, Fault, FolderProfile, FsEntry, ProfileSink,
    ProfileState, Tally, folder_profile, folder_size,
};

pub(super) struct Every<F>(pub(super) Duration, pub(super) F);

impl<F: FnMut(&FolderProfile) -> ControlFlow<()>> ProfileSink for Every<F> {
    fn snapshot(&mut self, profile: &FolderProfile) {
        let _ = (self.1)(profile);
    }

    fn every(&self) -> Duration {
        self.0
    }
}

fn file_at(name: &str, size: u64, modified_ms: Option<i64>) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind: EntryKind::File,
        size,
        modified_ms,
        attributes: Attributes::default(),
    }
}

fn tree() -> FakeFs {
    let fs = FakeFs::new();
    fs.add_file("d/a.bin", 100);
    fs.add_file("d/sub/b.TXT", 20);
    fs.add_file("d/sub/deeper/c.txt", 3);
    fs.add_file("d/sub/README", 7);
    fs.add_folder("d/empty");
    fs
}

fn profile(fs: &FakeFs, path: &str) -> FolderProfile {
    folder_profile(
        fs,
        &fs.location(path),
        &CancelToken::new(),
        &mut |_: &FolderProfile| {},
    )
}

fn kind<'a>(profile: &'a FolderProfile, extension: &str) -> Option<&'a Tally> {
    profile
        .kinds
        .iter()
        .find(|(name, _)| name == extension)
        .map(|(_, tally)| tally)
}

#[test]
fn totals_match_folder_size() {
    let fs = tree();
    let size = folder_size(&fs, &fs.location("d"), &CancelToken::new()).unwrap();
    let profile = profile(&fs, "d");
    assert_eq!(profile.totals, size);
    assert_eq!(profile.state, ProfileState::Done);
}

#[test]
fn kinds_are_case_insensitive_and_bare_names_have_no_extension() {
    let fs = tree();
    fs.add_file("d/.bashrc", 1);
    let profile = profile(&fs, "d");
    assert_eq!(
        kind(&profile, "txt"),
        Some(&Tally {
            bytes: 23,
            files: 2
        })
    );
    assert_eq!(kind(&profile, ""), Some(&Tally { bytes: 8, files: 2 }));
    assert_eq!(
        kind(&profile, "bin"),
        Some(&Tally {
            bytes: 100,
            files: 1
        })
    );
}

#[test]
fn kinds_are_biggest_first() {
    let profile = profile(&tree(), "d");
    let bytes: Vec<u64> = profile.kinds.iter().map(|(_, t)| t.bytes).collect();
    assert_eq!(bytes, vec![100, 23, 7]);
}

#[test]
fn kinds_beyond_the_cap_fold_into_other_keeping_the_biggest() {
    let fs = FakeFs::new();
    for index in 0..70_u64 {
        fs.add_file(&format!("d/f{index}.e{index}"), index + 1);
    }
    let profile = profile(&fs, "d");
    assert_eq!(profile.kinds.len(), 64);
    assert!(kind(&profile, "e69").is_some());
    assert!(kind(&profile, "e6").is_some());
    assert!(kind(&profile, "e5").is_none());
    assert_eq!(
        profile.other,
        Tally {
            bytes: 21,
            files: 6
        }
    );
}

#[test]
fn the_ten_largest_files_break_ties_by_path() {
    let fs = FakeFs::new();
    fs.add_file("d/big.bin", 50);
    fs.add_file("d/x/second.bin", 40);
    for name in ["e", "d", "c", "b", "a"] {
        fs.add_file(&format!("d/thirty/{name}.bin"), 30);
        fs.add_file(&format!("d/twenty/{name}.bin"), 20);
    }
    fs.add_file("d/tiny.bin", 1);
    let largest: Vec<(String, u64)> = profile(&fs, "d")
        .largest
        .into_iter()
        .map(|file| (file.path, file.bytes))
        .collect();
    let mut expected = vec![("big.bin".to_owned(), 50), ("x/second.bin".to_owned(), 40)];
    expected.extend(["a", "b", "c", "d", "e"].map(|n| (format!("thirty/{n}.bin"), 30)));
    expected.extend(["a", "b", "c"].map(|n| (format!("twenty/{n}.bin"), 20)));
    assert_eq!(largest, expected);
}

#[test]
fn progress_snapshots_arrive_and_only_grow() {
    let fs = tree().with_batch_size(1);
    let mut seen: Vec<FolderProfile> = Vec::new();
    let mut sink = Every(Duration::ZERO, |p: &FolderProfile| {
        seen.push(p.clone());
        ControlFlow::Continue(())
    });
    let last = folder_profile(&fs, &fs.location("d"), &CancelToken::new(), &mut sink);
    assert!(seen.len() > 2);
    assert_eq!(seen.last(), Some(&last));
    for pair in seen.windows(2) {
        let (a, b) = (&pair[0].totals, &pair[1].totals);
        assert!(a.bytes <= b.bytes && a.files <= b.files && a.folders <= b.folders);
    }
    let running = seen.iter().filter(|p| p.state == ProfileState::Running);
    assert_eq!(running.count(), seen.len() - 1);
}

#[test]
fn progress_is_throttled_to_the_interval() {
    let fs = tree().with_batch_size(1);
    let start = Instant::now();
    let mut times = vec![start];
    let mut sink = |p: &FolderProfile| {
        if p.state == ProfileState::Running {
            times.push(Instant::now());
        }
    };
    folder_profile(&fs, &fs.location("d"), &CancelToken::new(), &mut sink);
    for pair in times.windows(2) {
        assert!(pair[1] - pair[0] >= Duration::from_millis(100));
    }
}

#[test]
fn cancel_stops_early_and_reports_stopped() {
    let fs = FakeFs::new().with_batch_size(1);
    for index in 0..50 {
        fs.add_file(&format!("d/f{index}/x.bin"), 1);
    }
    let cancel = CancelToken::new();
    let mut sink = Every(Duration::ZERO, |_: &FolderProfile| {
        cancel.cancel();
        ControlFlow::Continue(())
    });
    let profile = folder_profile(&fs, &fs.location("d"), &cancel.clone(), &mut sink);
    assert_eq!(profile.state, ProfileState::Stopped);
    assert!(profile.totals.folders < 50);
    let listed = (0..50)
        .filter(|i| fs.calls(&format!("d/f{i}"), FakeOp::List) > 0)
        .count();
    assert!(listed <= 1);
}

#[test]
fn a_cancelled_start_is_stopped_with_nothing_counted() {
    let fs = tree();
    let cancel = CancelToken::new();
    cancel.cancel();
    let profile = folder_profile(&fs, &fs.location("d"), &cancel, &mut |_: &FolderProfile| {});
    assert_eq!(profile.state, ProfileState::Stopped);
    assert_eq!(profile.totals.files, 0);
}

#[test]
fn an_unreadable_subfolder_counts_as_skipped() {
    let fs = tree();
    fs.fault(
        "d/sub",
        FakeOp::List,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let profile = profile(&fs, "d");
    assert_eq!((profile.totals.bytes, profile.totals.skipped), (100, 1));
    assert_eq!(profile.state, ProfileState::Done);
}

#[test]
fn an_unreadable_top_folder_fails() {
    let fs = tree();
    fs.fault("d", FakeOp::List, 1, Fault::Fail(FsErrorKind::AccessDenied));
    let profile = profile(&fs, "d");
    assert_eq!(
        profile.state,
        ProfileState::Failed(FsErrorKind::AccessDenied)
    );
}

#[test]
fn the_newest_modification_is_the_newest_file() {
    let fs = FakeFs::new();
    fs.add_entry("d", file_at("old.txt", 1, Some(1_000)));
    fs.add_entry("d/sub", file_at("new.txt", 1, Some(9_000)));
    fs.add_entry("d", file_at("unknown.txt", 1, None));
    let mut folder = file_at("later", 0, Some(99_000));
    folder.kind = EntryKind::Folder;
    fs.add_entry("d", folder);
    assert_eq!(profile(&fs, "d").newest_ms, Some(9_000));
}

#[test]
fn links_and_junctions_are_counted_but_never_followed() {
    let fs = tree();
    let mut junction = file_at("loop", 0, None);
    junction.kind = EntryKind::Link;
    junction.attributes.link_to_folder = true;
    fs.add_entry("d", junction);
    fs.add_entry("d/loop", file_at("inside.bin", 5, None));
    let size = folder_size(&fs, &fs.location("d"), &CancelToken::new()).unwrap();
    let profile = profile(&fs, "d");
    assert_eq!(profile.totals, size);
    assert_eq!(fs.calls("d/loop", FakeOp::List), 0);
    assert_eq!(profile.state, ProfileState::Done);
}
