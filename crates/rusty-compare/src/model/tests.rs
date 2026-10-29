use std::collections::HashSet;
use std::io::Read;
use std::mem::size_of;
use std::ops::ControlFlow;
use std::sync::Arc;

use super::*;

const PAIRED_ROWS: usize = 100_000;
const PATH_BYTES: usize = 128;
const ROW_MEMORY_GOAL: usize = 50 * 1024 * 1024;

struct EmptySource {
    provider: ProviderId,
    root: ProviderPath,
}

impl SideSource for EmptySource {
    fn provider(&self) -> &ProviderId {
        &self.provider
    }

    fn list(
        &self,
        _folder: &RelPath,
        _sink: &mut dyn FnMut(SideEntry) -> ControlFlow<()>,
        _cancel: &CancelToken,
    ) -> Result<(), Reason> {
        Ok(())
    }

    fn open(&self, _rel: &RelPath, _hint: ReadHint) -> Result<Box<dyn Read + Send>, Reason> {
        Ok(Box::new(std::io::empty()))
    }

    fn object_id(&self, _rel: &RelPath) -> Option<ObjectId> {
        None
    }

    fn file_id(&self, _rel: &RelPath) -> Option<FileIdentity> {
        None
    }

    fn cache_path(&self, rel: &RelPath) -> ProviderPath {
        ProviderPath::from_text(&format!(
            "{}/{}",
            self.root.to_text(),
            rel.as_path().to_text()
        ))
    }

    fn case_sensitive(&self) -> bool {
        true
    }

    fn read_only(&self) -> bool {
        false
    }
}

struct NoCache;

impl HashCache for NoCache {
    fn get(&self, _key: &ContentKey) -> Option<[u8; 32]> {
        None
    }

    fn put(&self, _key: &ContentKey, _hash: [u8; 32]) {}
}

fn local() -> ProviderId {
    ProviderId::new("local").unwrap()
}

fn rel_path(path: ProviderPath) -> RelPath {
    RelPath::new(path)
}

fn entry(rel: &RelPath) -> SideEntry {
    SideEntry::new(rel.clone(), EntryKind::File, 1, None)
}

fn content_key(source: &dyn SideSource, entry: &SideEntry) -> ContentKey {
    ContentKey {
        provider: source.provider().clone(),
        full_path: source.cache_path(&entry.rel),
        size: entry.size,
        modified: entry.modified,
        file_id: source.file_id(&entry.rel),
    }
}

fn path_allocation_bytes(path: &ProviderPath) -> usize {
    if path.as_str().is_some() {
        return path.to_text().capacity();
    }
    if let Some(wide) = path.to_wide() {
        return wide.capacity() * size_of::<u16>();
    }
    path.to_bytes().map_or(0, |bytes| bytes.capacity())
}

#[test]
fn path_capacities_are_counted_for_every_representation() {
    let text = ProviderPath::from_text(&"a".repeat(PATH_BYTES));
    assert_eq!(path_allocation_bytes(&text), PATH_BYTES);

    let wide = ProviderPath::from_wide(&[0xD800; PATH_BYTES]);
    assert_eq!(path_allocation_bytes(&wide), PATH_BYTES * size_of::<u16>());

    let bytes = ProviderPath::from_bytes(&[0xFF; PATH_BYTES]);
    assert_eq!(path_allocation_bytes(&bytes), PATH_BYTES);
}

#[test]
fn a_row_stays_within_the_memory_goal() {
    let size = size_of::<Row>();
    assert!(size <= 96, "a row of {size} bytes breaks the 100k-row goal");
}

#[test]
fn a_hundred_thousand_paired_rows_fit_in_fifty_megabytes() {
    let text_rel = rel_path(ProviderPath::from_text(&format!(
        "{}/file.txt",
        "d".repeat(PATH_BYTES - "file.txt".len() - 1)
    )));
    let bytes_rel = rel_path(ProviderPath::from_bytes(&[0xFF; PATH_BYTES]));
    let mut wide_units = vec![u16::from(b'a'); PATH_BYTES / 2];
    wide_units[0] = 0xD800;
    let wide_rel = rel_path(ProviderPath::from_wide(&wide_units));
    assert_eq!(path_allocation_bytes(text_rel.as_path()), PATH_BYTES);
    assert_eq!(path_allocation_bytes(bytes_rel.as_path()), PATH_BYTES);
    assert_eq!(path_allocation_bytes(wide_rel.as_path()), PATH_BYTES);

    let mut rows = Vec::with_capacity(PAIRED_ROWS);
    for index in 0..PAIRED_ROWS {
        let left = if index % 2 == 0 {
            text_rel.clone()
        } else {
            bytes_rel.clone()
        };
        rows.push(Row {
            left: Some(Arc::new(entry(&left))),
            right: Some(Arc::new(entry(&wide_rel))),
            raw_status: Status::Same,
            display_status: Status::Same,
            generation: Generation::new(u64::try_from(index).unwrap()),
            content: ContentClass::Unknown,
        });
    }

    let retained_rows = rows.len() * size_of::<Row>();
    let retained_entries = rows
        .iter()
        .flat_map(|row| [row.left.as_deref(), row.right.as_deref()])
        .flatten()
        .map(|entry| {
            2 * size_of::<usize>()
                + size_of::<SideEntry>()
                + path_allocation_bytes(entry.rel.as_path())
        })
        .sum::<usize>();
    let retained = retained_rows + retained_entries;
    assert!(retained <= ROW_MEMORY_GOAL, "{retained} bytes retained");
}

#[test]
fn entry_names_are_the_last_canonical_segment() {
    let backslash = entry(&rel_path(ProviderPath::from_text(r"src/a\b.txt")));
    assert_eq!(backslash.name(), ProviderPath::from_text(r"a\b.txt"));

    let nested = entry(&rel_path(ProviderPath::from_text("dir/x.txt")));
    assert_eq!(nested.name(), ProviderPath::from_text("x.txt"));

    let top = entry(&rel_path(ProviderPath::from_text("x.txt")));
    assert_eq!(top.name(), ProviderPath::from_text("x.txt"));
}

#[test]
fn sources_with_different_roots_get_different_cache_keys() {
    let left = EmptySource {
        provider: local(),
        root: ProviderPath::from_text("left"),
    };
    let right = EmptySource {
        provider: local(),
        root: ProviderPath::from_text("right"),
    };
    let entry = entry(&rel_path(ProviderPath::from_text("a.txt")));

    let left_key = content_key(&left, &entry);
    let right_key = content_key(&right, &entry);
    assert_ne!(left_key, right_key);
    assert_eq!(left_key.full_path, ProviderPath::from_text("left/a.txt"));
    assert_eq!(right_key.full_path, ProviderPath::from_text("right/a.txt"));
}

#[test]
fn object_ids_keep_their_hash_length() {
    let sha1 = ObjectId::Sha1([0x11; 20]);
    assert_eq!(sha1.byte_len(), 20);
    assert_eq!(sha1.as_bytes(), &[0x11; 20]);

    let sha256 = ObjectId::Sha256([0x22; 32]);
    assert_eq!(sha256.byte_len(), 32);
    assert_eq!(sha256.as_bytes(), &[0x22; 32]);
}

#[test]
fn object_id_bytes_must_be_twenty_or_thirty_two_long() {
    for len in [0, 1, 19, 21, 31, 33] {
        assert_eq!(ObjectId::from_bytes(&vec![0; len]), None);
    }
    assert_eq!(
        ObjectId::from_bytes(&[0x33; 20]),
        Some(ObjectId::Sha1([0x33; 20]))
    );
    assert_eq!(
        ObjectId::from_bytes(&[0x44; 32]),
        Some(ObjectId::Sha256([0x44; 32]))
    );
}

#[test]
fn budgets_convert_the_setting_to_bytes() {
    assert_eq!(Budget::from_mib(Budget::MIN_MIB).bytes(), 256 * 1024 * 1024);
    assert_eq!(Budget::default().bytes(), 1024 * 1024 * 1024);
    assert_eq!(Budget::from_bytes(7).bytes(), 7);
}

#[test]
fn cancelling_is_visible_through_every_clone() {
    let token = CancelToken::new();
    let watcher = token.clone();
    assert!(!watcher.is_cancelled());
    token.cancel();
    assert!(watcher.is_cancelled());
}

#[test]
fn every_status_and_reason_has_its_own_message() {
    let statuses = [
        Status::Same,
        Status::SameAfterRules,
        Status::Different {
            first_difference: None,
        },
        Status::OnlyLeft,
        Status::OnlyRight,
        Status::TypeConflict,
        Status::CaseConflict,
        Status::Checking,
        Status::SameSizeTime,
        Status::NotChecked(Reason::Stopped),
        Status::Unavailable(Reason::ReadError),
    ];
    let reasons = [
        Reason::AccessDenied,
        Reason::NotFound,
        Reason::ReadError,
        Reason::Stopped,
        Reason::Link,
        Reason::Submodule,
        Reason::Unsupported,
        Reason::ChangedOnDisk,
    ];

    let mut ids = HashSet::new();
    for status in statuses {
        let id = status.fluent_id().as_str();
        assert!(id.starts_with("compare-status-"), "{id}");
        assert!(ids.insert(id), "duplicate message id {id}");
    }
    for reason in reasons {
        let id = reason.fluent_id().as_str();
        assert!(id.starts_with("compare-reason-"), "{id}");
        assert!(ids.insert(id), "duplicate message id {id}");
    }
}

#[test]
fn side_sources_and_hash_caches_are_usable_as_trait_objects() {
    let source = EmptySource {
        provider: local(),
        root: ProviderPath::from_text("root"),
    };
    let source: &dyn SideSource = &source;
    assert_eq!(source.provider().as_str(), "local");
    let entry = entry(&rel_path(ProviderPath::from_text("a.txt")));
    assert!(source.object_id(&entry.rel).is_none());

    let cache: &dyn HashCache = &NoCache;
    let key = content_key(source, &entry);
    assert_eq!(cache.get(&key), None);
}
