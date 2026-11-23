//! Snapshot persistence shared by the volume worker and rebuild lifecycle.

use std::{
    path::Path,
    sync::{Arc, PoisonError},
    time::Duration,
};

use navo_index::{
    file::{self, Header},
    ntfs::{NtfsIndex, SizeHint},
};

use super::{
    answer::Shared,
    config::Drive,
    lifecycle::{SaveClock, build_hint},
    rebuild::{SaveCursor, checked},
};

/// Inputs for one due-checked snapshot write: published index, drive, path, elapsed time, and
/// the journal cursor the rebuild state allows.
#[derive(Clone, Copy)]
pub(super) struct SaveInput<'a> {
    pub shared: &'a Shared,
    pub drive: Drive,
    pub path: &'a Path,
    pub elapsed: Duration,
    pub stopping: bool,
    pub built_at: i64,
    pub cursor: SaveCursor,
}

/// Returns the existing build-hint policy applied to the published index.
pub(super) fn rebuild_hint(shared: &Shared) -> SizeHint {
    build_hint(
        shared
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .index
            .as_deref(),
    )
}

/// Publishes a new index, drops the old generation, then trims the new table.
pub(super) fn replace(shared: &Shared, index: NtfsIndex) {
    let mut state = shared.write().unwrap_or_else(PoisonError::into_inner);
    state.status.progress = None;
    let old = state.index.replace(Arc::new(index));
    drop(old);
    // Trim only after the old index is gone, so the copy does not double the rebuild peak.
    if let Some(index) = state.index.as_mut().and_then(Arc::get_mut) {
        index.table.trim_slack();
    }
}

/// Streams a due published snapshot using elapsed time and epoch seconds; returns write errors.
/// Writes nothing when the cursor is `Skip` or not at or before the index's own cursor.
pub(super) fn save_snapshot(
    input: SaveInput<'_>,
    clock: &mut SaveClock,
) -> Result<Option<usize>, file::FileError> {
    let Some(index) = super::batch::published(input.shared) else {
        return Ok(None);
    };
    let snapshot = index.snapshot();
    if !clock.due(input.elapsed, snapshot.as_ref(), input.stopping) {
        return Ok(None);
    }
    let Some((table, ids, live)) = snapshot else {
        return Ok(None);
    };
    let Some(position) = checked(live, input.cursor) else {
        return Ok(None);
    };
    let header = Header {
        volume_serial: input.drive.serial,
        journal_id: position.journal_id,
        next_usn: position.next_usn,
        root: format!("{}:", input.drive.letter),
        built_at: input.built_at,
    };
    clock
        .save(input.path, (header, table, ids), input.elapsed)
        .map(|()| Some(table.len()))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, RwLock};

    use navo_index::{
        ntfs::NtfsIndex,
        table::{Flags, NO_PARENT, NameTable},
    };

    use super::{super::answer::State, *};

    /// Builds a published root and distinct drive metadata; invalid fixture data panics.
    fn fixture() -> (Shared, Header, Drive) {
        let header = Header {
            volume_serial: 42,
            journal_id: 7,
            next_usn: 100,
            root: "C:".into(),
            built_at: 1234,
        };
        let mut table = NameTable::new();
        table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
        let index = NtfsIndex::restore((header.clone(), table, [(5, 0)].into()), 42, 7).unwrap();
        let shared = Arc::new(RwLock::new(State {
            index: Some(Arc::new(index)),
            ..State::default()
        }));
        let drive = Drive {
            letter: 'C',
            serial: 99,
            fixed: true,
            ntfs: true,
        };
        (shared, header, drive)
    }

    #[test]
    fn persistence_saves_due_published_header_and_ids() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("volume.index");
        let (shared, header, drive) = fixture();
        let mut clock = SaveClock::default();
        clock.changed(Duration::ZERO);
        let input = SaveInput {
            shared: &shared,
            drive,
            path: &path,
            elapsed: Duration::from_secs(30),
            stopping: false,
            built_at: 5678,
            cursor: SaveCursor::Live,
        };
        assert_eq!(save_snapshot(input, &mut clock).unwrap(), Some(1));
        let (saved, table, ids) = file::load_with_ids(&path).unwrap();
        assert_eq!(saved.next_usn, header.next_usn);
        assert_eq!(
            saved,
            Header {
                volume_serial: 99,
                built_at: 5678,
                ..header
            }
        );
        assert_eq!((table.len(), ids.get(&5).copied()), (1, Some(0)));
    }

    #[test]
    fn persistence_waits_for_due_and_skips_unavailable_indexes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("volume.index");
        let (shared, _header, drive) = fixture();
        let mut clock = SaveClock::default();
        let mut input = SaveInput {
            shared: &shared,
            drive,
            path: &path,
            elapsed: Duration::ZERO,
            stopping: false,
            built_at: 5678,
            cursor: SaveCursor::Live,
        };
        assert_eq!(save_snapshot(input, &mut clock).unwrap(), None);
        clock.changed(Duration::ZERO);
        assert_eq!(save_snapshot(input, &mut clock).unwrap(), None);
        assert!(!path.exists());
        input.stopping = true;
        assert_eq!(save_snapshot(input, &mut clock).unwrap(), Some(1));
        clock.changed(Duration::ZERO);
        Arc::get_mut(shared.write().unwrap().index.as_mut().unwrap())
            .unwrap()
            .make_search_only();
        assert_eq!(save_snapshot(input, &mut clock).unwrap(), None);
        shared.write().unwrap().index = None;
        assert_eq!(save_snapshot(input, &mut clock).unwrap(), None);
    }

    #[test]
    fn persistence_backs_off_after_a_failed_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("volume.index");
        let (shared, _header, drive) = fixture();
        let mut clock = SaveClock::default();
        clock.changed(Duration::ZERO);
        let bad_path = path.join("blocked.index");
        let mut input = SaveInput {
            shared: &shared,
            drive,
            path: &bad_path,
            elapsed: Duration::from_secs(30),
            stopping: false,
            built_at: 5678,
            cursor: SaveCursor::Live,
        };
        assert!(save_snapshot(input, &mut clock).is_err());
        input.path = &path;
        input.elapsed = Duration::from_secs(59);
        assert_eq!(save_snapshot(input, &mut clock).unwrap(), None);
        input.elapsed = Duration::from_secs(60);
        assert_eq!(save_snapshot(input, &mut clock).unwrap(), Some(1));
    }
}

#[cfg(test)]
#[path = "rebuild_session_tests.rs"]
mod rebuild_session_tests;
