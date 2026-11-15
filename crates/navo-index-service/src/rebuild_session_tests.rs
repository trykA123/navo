//! Direct tests for the rebuild-session hint and actual-swap helpers.

use std::sync::{Arc, RwLock};

use navo_index::{
    file::Header,
    ntfs::NtfsIndex,
    table::{Flags, NO_PARENT, NameTable},
};

use super::super::{
    answer::{Shared, State},
    batch,
};
use super::{rebuild_hint, replace};

/// Builds a published root plus `files` distinct files; invalid fixture data panics.
fn index(prefix: &str, files: usize) -> NtfsIndex {
    let header = Header {
        volume_serial: 42,
        journal_id: 7,
        next_usn: 100,
        root: "C:".into(),
        built_at: 1234,
    };
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    for number in 0..files {
        table
            .push(root, &format!("{prefix}-{number:04}.txt"), Flags::NONE)
            .unwrap();
    }
    NtfsIndex::restore((header, table, [(5, 0)].into()), 42, 7).unwrap()
}

/// Publishes one index with no pending progress.
fn shared(index: NtfsIndex) -> Shared {
    Arc::new(RwLock::new(State {
        index: Some(Arc::new(index)),
        ..State::default()
    }))
}

#[test]
fn replace_publishes_the_new_index_and_leaves_old_readers_intact() {
    let shared = shared(index("old", 1));
    let old_reader = batch::published(&shared).unwrap();
    shared.write().unwrap().status.progress = Some(7);
    replace(&shared, index("new", 1));
    let current = batch::published(&shared).unwrap();
    assert_eq!(current.table.name(1), Some("new-0000.txt"));
    assert_eq!(old_reader.table.name(1), Some("old-0000.txt"));
    assert_eq!(shared.read().unwrap().status.progress, None);
}

#[test]
fn rebuild_hint_follows_the_published_index_before_and_after_replace() {
    let shared = shared(index("old", 2));
    let before = rebuild_hint(&shared);
    let old = batch::published(&shared).unwrap();
    assert_eq!(before.entries, old.table.len() + old.table.len() / 16);
    assert_eq!(
        before.folders,
        old.table.folder_count() + old.table.folder_count() / 16
    );
    assert_eq!(
        before.name_bytes,
        old.table.live_name_bytes() + old.table.live_name_bytes() / 16
    );
    replace(&shared, index("new", 32));
    let after = rebuild_hint(&shared);
    let new = batch::published(&shared).unwrap();
    assert_eq!(after.entries, new.table.len() + new.table.len() / 16);
    assert!(after.entries > before.entries);
}
