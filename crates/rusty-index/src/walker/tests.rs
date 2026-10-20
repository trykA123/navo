use std::collections::HashSet;
use std::sync::{Barrier, Mutex};
use std::thread;

use rusty_core::{FsError, FsErrorKind, Location, ProviderId, ProviderPath};
use rusty_fs::{
    Attributes, CancelToken, Capabilities, EntryKind, FakeFs, FakeOp, Fault, FsEntry, ListSink,
    Provider, hostile_names,
};

use super::{Limits, Outcome, WalkResult, walk};
use crate::table::Flags;

fn limits(max_entries: usize) -> Limits {
    Limits {
        max_entries,
        exclude: Vec::new(),
    }
}

fn run(fs: &dyn Provider, root: &Location) -> WalkResult {
    walk(fs, root, limits(100_000), &CancelToken::new(), |_, _| {}).unwrap()
}

fn entry(name: ProviderPath, kind: EntryKind, attributes: Attributes) -> FsEntry {
    FsEntry {
        name,
        kind,
        size: 0,
        modified_ms: None,
        attributes,
    }
}

fn paths(result: &WalkResult) -> HashSet<String> {
    result
        .table
        .iter()
        .map(|(id, _)| result.table.path_with(id, '/').unwrap())
        .collect()
}

#[test]
fn fake_tree_has_parents_and_breadth_first_order() {
    let fs = FakeFs::new().with_batch_size(1);
    fs.add_file("tree/a/deep/leaf.txt", 12);
    fs.add_file("tree/b/other.txt", 3);
    fs.add_folder("tree/empty");
    let result = run(&fs, &fs.location("tree"));
    assert_eq!(result.outcome, Outcome::Complete);
    assert_eq!(result.progress.entries, 7);
    assert_eq!(result.table.len(), 7);
    assert_eq!(
        paths(&result),
        HashSet::from(
            [
                "tree",
                "tree/a",
                "tree/b",
                "tree/empty",
                "tree/a/deep",
                "tree/b/other.txt",
                "tree/a/deep/leaf.txt",
            ]
            .map(str::to_owned)
        )
    );
    let depths: Vec<_> = result
        .table
        .iter()
        .map(|(id, _)| {
            result
                .table
                .path_with(id, '/')
                .unwrap()
                .matches('/')
                .count()
        })
        .collect();
    assert!(depths.windows(2).all(|pair| pair[0] <= pair[1]));
}

#[test]
fn copies_folder_hidden_and_system_flags() {
    let fs = FakeFs::new();
    fs.add_entry(
        "tree",
        entry(
            ProviderPath::from_text("cloud"),
            EntryKind::Folder,
            Attributes {
                hidden: true,
                system: true,
                ..Attributes::default()
            },
        ),
    );
    fs.add_file("tree/cloud/document.txt", 700);
    let result = run(&fs, &fs.location("tree"));
    let id = result
        .table
        .iter()
        .find(|(_, name)| *name == "cloud")
        .unwrap()
        .0;
    assert_eq!(
        result.table.flags(id),
        Some(Flags::FOLDER | Flags::HIDDEN | Flags::SYSTEM)
    );
    assert!(paths(&result).contains("tree/cloud/document.txt"));
    assert_eq!(fs.calls("tree/cloud/document.txt", FakeOp::Metadata), 0);
    assert_eq!(fs.calls("tree/cloud", FakeOp::Metadata), 0);
}

#[test]
fn hostile_names_round_trip_through_names_and_paths() {
    let fs = FakeFs::new();
    let corpus = hostile_names::corpus();
    for hostile in &corpus {
        fs.add_entry(
            "tree",
            entry(
                hostile.name.clone(),
                EntryKind::Folder,
                Attributes::default(),
            ),
        );
        let folder = fs.location("tree").join(&hostile.name).unwrap();
        fs.add_file(&format!("{}/leaf", folder.path().to_text()), 1);
    }
    let result = run(&fs, &fs.location("tree"));
    assert_eq!(result.table.len(), 1 + corpus.len() * 2);
    for hostile in corpus {
        let text = hostile.name.to_text();
        let id = result
            .table
            .iter()
            .find(|(_, name)| *name == text)
            .unwrap()
            .0;
        assert_eq!(
            ProviderPath::from_text(result.table.name(id).unwrap()),
            hostile.name
        );
        assert_eq!(
            result.table.path_with(id, '/'),
            Some(format!("tree/{text}"))
        );
        assert_eq!(fs.calls(&format!("tree/{text}"), FakeOp::List), 1);
    }
}

#[test]
fn links_are_recorded_without_following_folder_links_or_loops() {
    let fs = FakeFs::new();
    fs.add_file("tree/target/file", 1);
    for name in ["target-link", "loop"] {
        fs.add_entry(
            "tree",
            entry(
                ProviderPath::from_text(name),
                EntryKind::Link,
                Attributes {
                    link_to_folder: true,
                    ..Attributes::default()
                },
            ),
        );
    }
    fs.add_entry(
        "tree",
        entry(
            ProviderPath::from_text("file-link"),
            EntryKind::Link,
            Attributes::default(),
        ),
    );
    let provider = Probe {
        fs,
        list: |fs: &FakeFs, folder: &Location, sink: &mut dyn ListSink, cancel: &CancelToken| {
            let location = if folder.path().to_text().ends_with("/loop") {
                fs.location("tree")
            } else {
                folder.clone()
            };
            fs.list(&location, sink, cancel)
        },
    };
    let result = run(&provider, &provider.fs.location("tree"));
    assert_eq!(result.table.len(), 6);
    for name in ["target-link", "loop", "file-link"] {
        let id = result
            .table
            .iter()
            .find(|(_, found)| *found == name)
            .unwrap()
            .0;
        let flags = result.table.flags(id).unwrap();
        assert!(flags.contains(Flags::REPARSE));
        assert_eq!(flags.contains(Flags::FOLDER), name != "file-link");
        assert_eq!(provider.fs.calls(&format!("tree/{name}"), FakeOp::List), 0);
    }
    assert_eq!(provider.fs.calls("tree", FakeOp::List), 1);
}

#[test]
fn unreadable_folders_are_counted_and_siblings_continue() {
    let fs = FakeFs::new();
    fs.add_file("tree/denied/secret", 1);
    fs.add_file("tree/readable/file", 2);
    fs.fault(
        "tree/denied",
        FakeOp::List,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let result = run(&fs, &fs.location("tree"));
    assert_eq!(result.progress.skipped, 1);
    assert!(paths(&result).contains("tree/readable/file"));
    assert!(!paths(&result).contains("tree/denied/secret"));
    assert_eq!(result.outcome, Outcome::Complete);
}

#[test]
fn unreadable_root_is_recorded_and_counted() {
    let fs = FakeFs::new();
    fs.add_folder("tree");
    fs.fault(
        "tree",
        FakeOp::List,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let result = run(&fs, &fs.location("tree"));
    assert_eq!(result.table.len(), 1);
    assert_eq!(result.progress.skipped, 1);
    assert_eq!(result.outcome, Outcome::Complete);
}

#[test]
fn partial_listings_are_kept_when_a_folder_becomes_unreadable() {
    let fs = FakeFs::new();
    fs.add_file("tree/partial/first/file", 1);
    fs.add_file("tree/partial/second", 1);
    fs.fault(
        "tree/partial",
        FakeOp::List,
        1,
        Fault::FailAfterEntries {
            kind: FsErrorKind::AccessDenied,
            entries: 1,
        },
    );
    let result = run(&fs, &fs.location("tree"));
    assert_eq!(result.progress.skipped, 1);
    assert!(paths(&result).contains("tree/partial/first/file"));
    assert!(!paths(&result).contains("tree/partial/second"));
}

#[test]
fn already_cancelled_walk_does_not_list_or_add_entries() {
    let fs = FakeFs::new();
    let cancel = CancelToken::new();
    cancel.cancel();
    let result = walk(&fs, &fs.location("tree"), limits(10), &cancel, |_, _| {}).unwrap();
    assert_eq!(result.outcome, Outcome::Cancelled);
    assert!(result.table.is_empty());
    assert_eq!(fs.calls("tree", FakeOp::List), 0);
}

#[test]
fn entry_cap_includes_the_root_and_never_overshoots() {
    let fs = FakeFs::new();
    for i in 0..40 {
        fs.add_file(&format!("tree/folder-{i}/file"), 1);
    }
    for cap in [0, 1, 10, 45] {
        let result = walk(
            &fs,
            &fs.location("tree"),
            limits(cap),
            &CancelToken::new(),
            |_, _| {},
        )
        .unwrap();
        assert_eq!(result.table.len(), cap);
        assert_eq!(result.progress.entries, cap);
        assert_eq!(result.outcome, Outcome::EntryLimit);
    }
}

#[test]
fn excludes_names_and_exact_paths_before_recording_or_descending() {
    let fs = FakeFs::new();
    for name in ["skip", "a/skip", "a/omit", "b/omit", "kept"] {
        fs.add_file(&format!("tree/{name}/file"), 1);
    }
    let mut options = limits(100);
    options.exclude = vec!["skip".into(), "tree/a/omit".into()];
    let result = walk(
        &fs,
        &fs.location("tree"),
        options,
        &CancelToken::new(),
        |_, _| {},
    )
    .unwrap();
    let found = paths(&result);
    assert!(found.contains("tree/b/omit/file") && found.contains("tree/kept/file"));
    for skipped in ["tree/skip", "tree/a/skip", "tree/a/omit"] {
        assert!(!found.contains(skipped));
        assert_eq!(fs.calls(skipped, FakeOp::List), 0);
    }
}

#[test]
fn depth_256_is_recorded_but_never_listed() {
    let fs = FakeFs::new();
    let mut path = "tree".to_owned();
    for _ in 0..257 {
        path.push_str("/d");
    }
    fs.add_file(&format!("{path}/leaf"), 1);
    let result = run(&fs, &fs.location("tree"));
    assert_eq!(result.table.len(), 257);
    assert_eq!(result.progress.depth_limited, 1);
    assert_eq!(result.outcome, Outcome::Complete);
    let boundary = path.rsplit_once('/').unwrap().0;
    assert!(paths(&result).contains(boundary));
    assert_eq!(fs.calls(boundary, FakeOp::List), 0);
}

fn wide_tree() -> FakeFs {
    let fs = FakeFs::new().with_batch_size(25_000);
    for i in 0..20_005 {
        fs.add_file(&format!("tree/file-{i}"), 1);
    }
    fs
}

#[test]
fn progress_reports_every_10_000_entries_and_the_final_partial_table() {
    let fs = wide_tree();
    let mut reports = Vec::new();
    let result = walk(
        &fs,
        &fs.location("tree"),
        limits(30_000),
        &CancelToken::new(),
        |table, progress| {
            assert_eq!(table.len(), progress.entries);
            reports.push(progress);
        },
    )
    .unwrap();
    assert_eq!(
        reports.iter().map(|p| p.entries).collect::<Vec<_>>(),
        vec![10_000, 20_000, 20_006]
    );
    assert_eq!(reports.last(), Some(&result.progress));
}

#[test]
fn cancellation_from_progress_stops_inside_a_large_batch() {
    let fs = wide_tree();
    let cancel = CancelToken::new();
    let result = walk(
        &fs,
        &fs.location("tree"),
        limits(30_000),
        &cancel,
        |_, _| cancel.cancel(),
    )
    .unwrap();
    assert_eq!(result.outcome, Outcome::Cancelled);
    assert_eq!(result.table.len(), 10_000);
    assert_eq!(result.progress.skipped, 0);
}

#[test]
fn four_workers_list_sibling_folders_concurrently() {
    let fs = FakeFs::new();
    for i in 0..4 {
        fs.add_file(&format!("tree/d{i}/leaf"), 1);
    }
    let barrier = Barrier::new(4);
    let threads = Mutex::new(HashSet::new());
    let provider = Probe {
        fs,
        list: |fs: &FakeFs, folder: &Location, sink: &mut dyn ListSink, cancel: &CancelToken| {
            if folder.path().to_text() != "tree" {
                threads.lock().unwrap().insert(thread::current().id());
                barrier.wait();
            }
            fs.list(folder, sink, cancel)
        },
    };
    let result = run(&provider, &provider.fs.location("tree"));
    assert_eq!(threads.into_inner().unwrap().len(), 4);
    assert_eq!(result.table.len(), 9);
}

struct Probe<F> {
    fs: FakeFs,
    list: F,
}

impl<F> Provider for Probe<F>
where
    F: Fn(&FakeFs, &Location, &mut dyn ListSink, &CancelToken) -> Result<(), FsError> + Send + Sync,
{
    fn id(&self) -> ProviderId {
        self.fs.id()
    }
    fn capabilities(&self) -> Capabilities {
        self.fs.capabilities()
    }
    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.fs.metadata(location)
    }
    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        (self.list)(&self.fs, folder, sink, cancel)
    }
}
