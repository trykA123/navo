use std::collections::HashMap;
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::ops::ControlFlow;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use navo_core::{FsError, FsErrorKind, Location, ProviderPath};
use tempfile::TempDir;

use super::{
    DupesError, DupesRead, DupesSource, DuplicateGroup, DuplicateOptions, DuplicateSummary,
    NetworkScope, PARTIAL_HASH_BYTES, find_duplicates, find_duplicates_with,
};
use crate::{
    Attributes, CancelToken, DriveClass, EntryBatch, EntryKind, FileIdentity, FileIdentityKind,
    FsEntry, ListSink,
};

struct FakeNode {
    entry: FsEntry,
    identity: Option<FileIdentity>,
    bytes: Vec<u8>,
}

struct CountingReader {
    inner: Cursor<Vec<u8>>,
    bytes_read: Arc<AtomicUsize>,
    cancel_after_bytes: Option<(usize, CancelToken)>,
}

impl Read for CountingReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(buf)?;
        let total = self.bytes_read.fetch_add(count, Ordering::SeqCst) + count;
        if let Some((limit, token)) = &self.cancel_after_bytes
            && total >= *limit
        {
            token.cancel();
        }
        Ok(count)
    }
}

impl Seek for CountingReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}

struct FakeDupesSource {
    class: DriveClass,
    folders: HashMap<Location, Vec<FsEntry>>,
    files: HashMap<Location, FakeNode>,
    list_calls: AtomicUsize,
    identity_calls: AtomicUsize,
    open_calls: AtomicUsize,
    bytes_read: Arc<AtomicUsize>,
    cancel_after_bytes: Option<(usize, CancelToken)>,
    cancel_on_list_call: Option<(usize, CancelToken)>,
}

impl FakeDupesSource {
    fn new(class: DriveClass) -> Self {
        Self {
            class,
            folders: HashMap::new(),
            files: HashMap::new(),
            list_calls: AtomicUsize::new(0),
            identity_calls: AtomicUsize::new(0),
            open_calls: AtomicUsize::new(0),
            bytes_read: Arc::new(AtomicUsize::new(0)),
            cancel_after_bytes: None,
            cancel_on_list_call: None,
        }
    }

    fn add_folder(&mut self, folder: &Location) {
        self.folders.entry(folder.clone()).or_default();
        if let Some(parent) = folder.parent()
            && self.folders.contains_key(&parent)
            && let Some(name) = leaf_name(folder)
        {
            let entry = FsEntry {
                name,
                kind: EntryKind::Folder,
                size: 0,
                modified_ms: Some(1_000),
                attributes: Attributes::default(),
            };
            self.folders.entry(parent).or_default().push(entry);
        }
    }

    fn add_file(
        &mut self,
        parent: &Location,
        name: &str,
        bytes: Vec<u8>,
        identity: Option<FileIdentity>,
        attributes: Attributes,
    ) -> Location {
        let path = ProviderPath::from_text(name);
        let location = parent.join(&path).unwrap();
        let entry = FsEntry {
            name: path,
            kind: EntryKind::File,
            size: u64::try_from(bytes.len()).unwrap(),
            modified_ms: Some(2_000),
            attributes,
        };
        self.folders
            .entry(parent.clone())
            .or_default()
            .push(entry.clone());
        self.files.insert(
            location.clone(),
            FakeNode {
                entry,
                identity,
                bytes,
            },
        );
        location
    }

    fn add_link(&mut self, parent: &Location, name: &str, link_to_folder: bool) -> Location {
        let path = ProviderPath::from_text(name);
        let location = parent.join(&path).unwrap();
        let entry = FsEntry {
            name: path,
            kind: EntryKind::Link,
            size: 0,
            modified_ms: Some(2_000),
            attributes: Attributes {
                link_to_folder,
                ..Attributes::default()
            },
        };
        self.folders.entry(parent.clone()).or_default().push(entry);
        location
    }

    fn list_calls(&self) -> usize {
        self.list_calls.load(Ordering::SeqCst)
    }

    fn open_calls(&self) -> usize {
        self.open_calls.load(Ordering::SeqCst)
    }

    fn bytes_read(&self) -> usize {
        self.bytes_read.load(Ordering::SeqCst)
    }
}

impl DupesSource for FakeDupesSource {
    fn drive_class(&self, _root: &Location) -> DriveClass {
        self.class
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        let call = self.list_calls.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some((at, token)) = &self.cancel_on_list_call
            && call >= *at
        {
            token.cancel();
        }
        if cancel.is_cancelled() {
            return Err(FsError::new(FsErrorKind::Cancelled));
        }
        let entries = self.folders.get(folder).cloned().ok_or_else(|| FsError {
            location: Some(folder.clone()),
            ..FsError::new(FsErrorKind::NotFound)
        })?;
        let _ = sink.batch(EntryBatch {
            entries,
            last: true,
        });
        Ok(())
    }

    fn file_identity(&self, location: &Location) -> Result<Option<FileIdentity>, FsError> {
        self.identity_calls.fetch_add(1, Ordering::SeqCst);
        let node = self.files.get(location).ok_or_else(|| FsError {
            location: Some(location.clone()),
            ..FsError::new(FsErrorKind::NotFound)
        })?;
        assert!(
            !node.entry.attributes.cloud_placeholder,
            "file_identity must never be called on a cloud placeholder"
        );
        Ok(node.identity)
    }

    fn open_read(&self, location: &Location) -> Result<Box<dyn DupesRead>, FsError> {
        self.open_calls.fetch_add(1, Ordering::SeqCst);
        let node = self.files.get(location).ok_or_else(|| FsError {
            location: Some(location.clone()),
            ..FsError::new(FsErrorKind::NotFound)
        })?;
        Ok(Box::new(CountingReader {
            inner: Cursor::new(node.bytes.clone()),
            bytes_read: Arc::clone(&self.bytes_read),
            cancel_after_bytes: self.cancel_after_bytes.clone(),
        }))
    }
}

fn fake_root(text: &str) -> Location {
    Location::local(ProviderPath::from_text(text)).unwrap()
}

fn leaf_name(location: &Location) -> Option<ProviderPath> {
    let text = location.as_str().trim_end_matches(['/', '\\']);
    let (_, tail) = text.rsplit_once(['/', '\\'])?;
    (!tail.is_empty()).then(|| ProviderPath::from_text(tail))
}

fn native_id(volume: u64, id: u128) -> FileIdentity {
    FileIdentity {
        volume,
        file: id.to_le_bytes(),
        kind: FileIdentityKind::Native,
    }
}

fn local_location(path: &Path) -> Location {
    Location::local(ProviderPath::from_text(&path.to_string_lossy())).unwrap()
}

fn collect_groups(
    source: &dyn DupesSource,
    root: &Location,
    options: &DuplicateOptions,
    cancel: &CancelToken,
) -> Result<(Vec<DuplicateGroup>, DuplicateSummary), DupesError> {
    let mut groups = Vec::new();
    let summary = find_duplicates_with(source, root, options, cancel, |group| {
        groups.push(group);
        ControlFlow::Continue(())
    })?;
    Ok((groups, summary))
}

#[test]
fn same_size_and_head_tail_with_different_middle_is_not_a_duplicate() {
    let root = fake_root("/scan");
    let mut source = FakeDupesSource::new(DriveClass::Ssd);
    source.add_folder(&root);

    let size = PARTIAL_HASH_BYTES * 4 + 512;
    let file_a = vec![0xAB_u8; size];
    let mut file_b = file_a.clone();
    file_b[size / 2] ^= 0xFF;

    source.add_file(
        &root,
        "a.bin",
        file_a,
        Some(native_id(1, 1)),
        Attributes::default(),
    );
    source.add_file(
        &root,
        "b.bin",
        file_b,
        Some(native_id(1, 2)),
        Attributes::default(),
    );

    let (groups, summary) = collect_groups(
        &source,
        &root,
        &DuplicateOptions::default(),
        &CancelToken::new(),
    )
    .unwrap();

    assert!(
        groups.is_empty(),
        "files differing in the middle must not form a duplicate group"
    );
    assert_eq!(summary.scanned_files, 2);
    assert_eq!(summary.duplicate_groups, 0);
    assert!(
        source.open_calls() >= 2,
        "matching partial hashes must proceed to full BLAKE3 comparison"
    );
}

#[test]
fn identical_files_larger_than_16_kib_produce_one_duplicate_group_with_exact_blake3() {
    let root = fake_root("/scan");
    let sub = root.join(&ProviderPath::from_text("nested")).unwrap();
    let mut source = FakeDupesSource::new(DriveClass::Ssd);
    source.add_folder(&root);
    source.add_folder(&sub);

    let payload: Vec<u8> = (0..(20 * 1024))
        .map(|i| u8::try_from((i * 31 + 7) % 251).unwrap())
        .collect();
    let expected_hash = *blake3::hash(&payload).as_bytes();
    let expected_size = u64::try_from(payload.len()).unwrap();

    let first = source.add_file(
        &root,
        "alpha.dat",
        payload.clone(),
        Some(native_id(1, 10)),
        Attributes::default(),
    );
    let second = source.add_file(
        &sub,
        "beta.dat",
        payload,
        Some(native_id(1, 11)),
        Attributes::default(),
    );
    source.add_file(
        &root,
        "unique.dat",
        vec![1, 2, 3],
        Some(native_id(1, 12)),
        Attributes::default(),
    );

    let (groups, summary) = collect_groups(
        &source,
        &root,
        &DuplicateOptions::default(),
        &CancelToken::new(),
    )
    .unwrap();

    assert_eq!(groups.len(), 1);
    let group = &groups[0];
    assert_eq!(group.size, expected_size);
    assert_eq!(group.hash, expected_hash);
    assert_eq!(group.reclaimable_bytes(), expected_size);
    let mut paths: Vec<Location> = group.entries.iter().map(|e| e.location.clone()).collect();
    paths.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    let mut expected = vec![first, second];
    expected.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    assert_eq!(paths, expected);
    assert_eq!(summary.scanned_files, 3);
    assert_eq!(summary.duplicate_groups, 1);
    assert_eq!(summary.duplicate_files, 2);
    assert_eq!(summary.reclaimable_bytes, expected_size);
}

#[test]
fn cloud_placeholder_is_skipped_with_zero_open_or_read_calls() {
    let root = fake_root("/onedrive");
    let mut source = FakeDupesSource::new(DriveClass::Ssd);
    source.add_folder(&root);

    let payload = vec![0x42_u8; 16 * 1024];
    source.add_file(
        &root,
        "stub.bin",
        payload.clone(),
        Some(native_id(1, 1)),
        Attributes {
            cloud_placeholder: true,
            ..Attributes::default()
        },
    );
    source.add_file(
        &root,
        "local.bin",
        payload,
        Some(native_id(1, 2)),
        Attributes::default(),
    );

    let (groups, summary) = collect_groups(
        &source,
        &root,
        &DuplicateOptions::default(),
        &CancelToken::new(),
    )
    .unwrap();

    assert!(groups.is_empty());
    assert_eq!(summary.skipped_placeholders, 1);
    assert_eq!(
        source.open_calls(),
        0,
        "cloud placeholder must never be opened and sole remaining file needs no read"
    );
    assert_eq!(
        source.bytes_read(),
        0,
        "zero bytes may be read from cloud placeholder"
    );
}

#[test]
fn hard_links_sharing_file_identity_produce_zero_groups_and_count_skipped() {
    let root = fake_root("/data");
    let mut source = FakeDupesSource::new(DriveClass::Ssd);
    source.add_folder(&root);

    let shared_id = native_id(42, 999);
    let payload = vec![0x77_u8; 12 * 1024];
    source.add_file(
        &root,
        "orig.bin",
        payload.clone(),
        Some(shared_id),
        Attributes::default(),
    );
    source.add_file(
        &root,
        "hardlink.bin",
        payload,
        Some(shared_id),
        Attributes::default(),
    );

    let (groups, summary) = collect_groups(
        &source,
        &root,
        &DuplicateOptions::default(),
        &CancelToken::new(),
    )
    .unwrap();

    assert!(
        groups.is_empty(),
        "hard links to the same inode must never be reported as duplicates"
    );
    assert_eq!(summary.skipped_hard_links, 1);
    assert_eq!(
        source.open_calls(),
        0,
        "deduplicated single-inode bucket must be dropped before hashing"
    );
}

#[test]
fn network_root_is_refused_without_crawling_any_directory() {
    let root = fake_root("/mnt/nas");
    let mut source = FakeDupesSource::new(DriveClass::Network);
    source.add_folder(&root);
    source.add_file(
        &root,
        "a.bin",
        vec![1; 1024],
        Some(native_id(1, 1)),
        Attributes::default(),
    );
    source.add_file(
        &root,
        "b.bin",
        vec![1; 1024],
        Some(native_id(1, 2)),
        Attributes::default(),
    );

    let result = collect_groups(
        &source,
        &root,
        &DuplicateOptions::default(),
        &CancelToken::new(),
    );

    assert!(matches!(result, Err(DupesError::NetworkRootRefused(ref loc)) if loc == &root));
    assert_eq!(
        source.list_calls(),
        0,
        "network root refusal must happen before crawling any directory"
    );
    assert_eq!(source.open_calls(), 0);
}

#[test]
fn network_bounded_subfolder_opt_in_enforces_max_items_cap() {
    let root = fake_root("/mnt/nas/project");
    let sub = root.join(&ProviderPath::from_text("deep")).unwrap();
    let mut source = FakeDupesSource::new(DriveClass::Network);
    source.add_folder(&root);
    source.add_folder(&sub);

    for idx in 0..10_u128 {
        source.add_file(
            &root,
            &format!("f{idx}.bin"),
            vec![0x55; 1024],
            Some(native_id(2, idx + 1)),
            Attributes::default(),
        );
    }
    source.add_file(
        &sub,
        "never_seen.bin",
        vec![0x55; 1024],
        Some(native_id(2, 99)),
        Attributes::default(),
    );

    let options = DuplicateOptions {
        network_scope: NetworkScope::BoundedSubfolder { max_items: 4 },
        ..DuplicateOptions::default()
    };
    let (groups, summary) = collect_groups(&source, &root, &options, &CancelToken::new()).unwrap();

    assert!(
        summary.truncated_by_cap,
        "walk must record when max_items cap stops traversal"
    );
    assert!(
        summary.scanned_files + summary.scanned_folders <= 4,
        "visited items must not exceed max_items"
    );
    assert!(
        groups.iter().all(|g| g.entries.len() <= 4),
        "duplicate groups must only include entries within the cap"
    );

    let zero_cap = DuplicateOptions {
        network_scope: NetworkScope::BoundedSubfolder { max_items: 0 },
        ..DuplicateOptions::default()
    };
    assert!(matches!(
        collect_groups(&source, &root, &zero_cap, &CancelToken::new()),
        Err(DupesError::InvalidNetworkCap)
    ));
}

#[test]
fn symlink_entries_are_skipped_without_following() {
    let root = fake_root("/tree");
    let linked_folder = root.join(&ProviderPath::from_text("folder_link")).unwrap();
    let mut source = FakeDupesSource::new(DriveClass::Ssd);
    source.add_folder(&root);
    // Populate a folder under the link path that must never be listed.
    source.folders.insert(linked_folder.clone(), Vec::new());
    source.add_file(
        &linked_folder,
        "hidden_dup.bin",
        vec![0x99; 4096],
        Some(native_id(1, 50)),
        Attributes::default(),
    );
    source.add_link(&root, "folder_link", true);
    source.add_link(&root, "file_link", false);
    source.add_file(
        &root,
        "real.bin",
        vec![0x99; 4096],
        Some(native_id(1, 51)),
        Attributes::default(),
    );

    let (groups, summary) = collect_groups(
        &source,
        &root,
        &DuplicateOptions::default(),
        &CancelToken::new(),
    )
    .unwrap();

    assert!(groups.is_empty());
    assert_eq!(summary.skipped_links, 2);
    assert_eq!(
        source.list_calls(),
        1,
        "only root may be listed; folder link must not be followed"
    );
    assert_eq!(source.open_calls(), 0);
}

#[test]
fn cancellation_mid_scan_stops_promptly_without_panicking() {
    let root = fake_root("/large");
    let mut source = FakeDupesSource::new(DriveClass::Ssd);
    source.add_folder(&root);

    let cancel = CancelToken::new();
    source.cancel_after_bytes = Some((4096, cancel.clone()));

    let payload = vec![0x33_u8; 512 * 1024];
    source.add_file(
        &root,
        "one.bin",
        payload.clone(),
        Some(native_id(1, 1)),
        Attributes::default(),
    );
    source.add_file(
        &root,
        "two.bin",
        payload,
        Some(native_id(1, 2)),
        Attributes::default(),
    );

    let result = collect_groups(&source, &root, &DuplicateOptions::default(), &cancel);
    assert!(matches!(result, Err(DupesError::Cancelled)));

    // Also verify cancellation during Stage 1 walk.
    let walk_cancel = CancelToken::new();
    let sub = root.join(&ProviderPath::from_text("sub")).unwrap();
    let mut walk_source = FakeDupesSource::new(DriveClass::Ssd);
    walk_source.add_folder(&root);
    walk_source.add_folder(&sub);
    walk_source.cancel_on_list_call = Some((1, walk_cancel.clone()));
    let walk_result = collect_groups(
        &walk_source,
        &root,
        &DuplicateOptions::default(),
        &walk_cancel,
    );
    assert!(matches!(walk_result, Err(DupesError::Cancelled)));
}

#[test]
fn small_files_under_8_kib_and_zero_byte_policy_work_cleanly() {
    let root = fake_root("/small");
    let mut source = FakeDupesSource::new(DriveClass::Ssd);
    source.add_folder(&root);

    source.add_file(
        &root,
        "empty_a",
        Vec::new(),
        Some(native_id(1, 1)),
        Attributes::default(),
    );
    source.add_file(
        &root,
        "empty_b",
        Vec::new(),
        Some(native_id(1, 2)),
        Attributes::default(),
    );
    source.add_file(
        &root,
        "tiny_a",
        b"hello!".to_vec(),
        Some(native_id(1, 3)),
        Attributes::default(),
    );
    source.add_file(
        &root,
        "tiny_b",
        b"hello!".to_vec(),
        Some(native_id(1, 4)),
        Attributes::default(),
    );
    source.add_file(
        &root,
        "tiny_c",
        b"world!".to_vec(),
        Some(native_id(1, 5)),
        Attributes::default(),
    );

    let (groups, summary) = collect_groups(
        &source,
        &root,
        &DuplicateOptions::default(),
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].size, 6);
    assert_eq!(summary.skipped_small, 2);

    let include_empty = DuplicateOptions {
        min_size: 0,
        ..DuplicateOptions::default()
    };
    let (groups_with_empty, summary_with_empty) =
        collect_groups(&source, &root, &include_empty, &CancelToken::new()).unwrap();
    assert_eq!(groups_with_empty.len(), 2);
    assert_eq!(summary_with_empty.skipped_small, 0);
}

#[test]
fn local_fs_integration_finds_duplicates_and_skips_hard_links_and_symlinks() {
    let temp = TempDir::new().unwrap();
    let a = temp.path().join("a.bin");
    let b = temp.path().join("b.bin");
    let hard = temp.path().join("a_hard.bin");
    let diff = temp.path().join("diff_mid.bin");

    let mut payload = vec![0x11_u8; 20 * 1024];
    payload[0] = 0xAA;
    payload[20 * 1024 - 1] = 0xBB;
    std::fs::write(&a, &payload).unwrap();
    std::fs::write(&b, &payload).unwrap();
    std::fs::hard_link(&a, &hard).unwrap();

    let mut diff_payload = payload.clone();
    diff_payload[10 * 1024] = 0xFF;
    std::fs::write(&diff, &diff_payload).unwrap();

    #[cfg(unix)]
    {
        let sym = temp.path().join("sym.bin");
        std::os::unix::fs::symlink(&a, &sym).unwrap();
    }

    let root = local_location(temp.path());
    let mut groups = Vec::new();
    let summary = find_duplicates(
        &root,
        &DuplicateOptions::default(),
        &CancelToken::new(),
        |g| {
            groups.push(g);
            ControlFlow::Continue(())
        },
    )
    .unwrap();

    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].entries.len(),
        2,
        "hard link and symlink must not inflate the group"
    );
    assert_eq!(groups[0].hash, *blake3::hash(&payload).as_bytes());
    assert_eq!(summary.skipped_hard_links, 1);
    #[cfg(unix)]
    assert_eq!(summary.skipped_links, 1);
}

#[test]
fn fingerprint_identities_are_not_mistaken_for_hard_links_and_break_stops_streaming() {
    let root = fake_root("/fat");
    let mut source = FakeDupesSource::new(DriveClass::Removable);
    source.add_folder(&root);

    let fp_a = FileIdentity::fingerprint(1024, Some(2_000), false);
    let fp_b = FileIdentity::fingerprint(2048, Some(2_000), false);
    source.add_file(
        &root,
        "a1.bin",
        vec![1; 1024],
        Some(fp_a),
        Attributes::default(),
    );
    source.add_file(
        &root,
        "a2.bin",
        vec![1; 1024],
        Some(fp_a),
        Attributes::default(),
    );
    source.add_file(
        &root,
        "b1.bin",
        vec![2; 2048],
        Some(fp_b),
        Attributes::default(),
    );
    source.add_file(
        &root,
        "b2.bin",
        vec![2; 2048],
        Some(fp_b),
        Attributes::default(),
    );

    let mut streamed = Vec::new();
    let summary = find_duplicates_with(
        &source,
        &root,
        &DuplicateOptions::default(),
        &CancelToken::new(),
        |group| {
            streamed.push(group);
            ControlFlow::Break(())
        },
    )
    .unwrap();

    assert_eq!(
        streamed.len(),
        1,
        "ControlFlow::Break must stop streaming after the first group"
    );
    assert_eq!(
        streamed[0].size, 2048,
        "largest duplicate group streams first"
    );
    assert_eq!(
        summary.skipped_hard_links, 0,
        "fingerprint identities must not suppress duplicates"
    );
    assert_eq!(summary.duplicate_groups, 1);
}
