use std::time::Duration;

use navo_index::file::{self, Header};
use navo_index::ntfs::{NtfsIndex, SizeHint};
use navo_index::pipe::BuildReason;
use navo_index::table::{Flags, NO_PARENT, NameTable};
use navo_index::usn::{FrnMap, UsnBatch, UsnRecord};
use navo_store::Search;

const DRIVE: Drive = Drive {
    letter: 'C',
    serial: 42,
    fixed: true,
    ntfs: true,
};
use super::config::{Config, Drive, index_path, select_volumes};
use super::lifecycle::{SaveClock, build_hint, hint_from_counts, restore_or_enumerate};

#[test]
fn config_accepts_only_account_sids() {
    let settings = Search::default();
    let config = Config::new("S-1-5-21-1-2-3-1001".into(), settings.clone()).unwrap();
    let encoded = serde_json::to_vec(&config).unwrap();
    assert_eq!(Config::decode(&encoded).unwrap().search, settings);
    for sid in [
        "S-1-5-18",
        "S-1-5-32-544",
        "S-1-5-21-1-2-3-512",
        "S-1-5-21-1-2-3-4294967296",
        "S-1-5-21-1-2-3",
        "S-1-5-21-1-2-3-4-5",
        "S-1-5-21-1-2-3-4)(A;;GA;;;WD)",
    ] {
        assert!(Config::new(sid.into(), settings.clone()).is_err());
    }
    assert!(Config::decode(br#"{"sid":"S-1-5-18","search":{}}"#).is_err());
}

#[test]
fn volume_defaults_use_only_fixed_ntfs_and_explicit_roots_are_validated() {
    let available = [
        Drive {
            letter: 'C',
            serial: 1,
            fixed: true,
            ntfs: true,
        },
        Drive {
            letter: 'D',
            serial: 2,
            fixed: true,
            ntfs: false,
        },
        Drive {
            letter: 'E',
            serial: 3,
            fixed: false,
            ntfs: true,
        },
    ];
    assert_eq!(
        select_volumes(&Search::default(), &available).unwrap(),
        vec![available[0]]
    );
    let mut search = Search {
        drives: vec!["c:\\".into(), "C:".into()],
        ..Search::default()
    };
    assert_eq!(
        select_volumes(&search, &available).unwrap(),
        vec![available[0]]
    );
    search.drives = vec!["E:".into()];
    assert!(select_volumes(&search, &available).unwrap().is_empty());
    search.removable = true;
    assert_eq!(
        select_volumes(&search, &available).unwrap(),
        vec![available[2]]
    );
    for root in ["C:\\users", "\\\\host\\share", "../outside"] {
        search.drives = vec![root.into()];
        assert!(select_volumes(&search, &available).is_err());
    }
}

#[test]
fn index_paths_stay_in_the_programdata_index_folder() {
    let base = std::path::Path::new("program-data");
    assert_eq!(
        index_path(base, 'c').unwrap(),
        base.join("Navo/index/C.index")
    );
    assert!(index_path(base, '/').is_err());
}

fn snapshot() -> (Header, NameTable, navo_index::usn::FrnMap) {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    (
        Header {
            volume_serial: 42,
            journal_id: 7,
            next_usn: 100,
            root: "C:".into(),
            built_at: 0,
        },
        table,
        [(5, root)].into(),
    )
}

#[test]
fn valid_snapshot_restores_and_resumes_without_enumeration() {
    let mut result = restore_or_enumerate(Ok(snapshot()), DRIVE, |_| {
        panic!("a valid snapshot must resume")
    })
    .unwrap();
    assert_eq!(result.position.next_usn, 100);
    result
        .apply_batch(UsnBatch {
            next_usn: 110,
            records: vec![UsnRecord {
                size: None,
                file_id: 9,
                parent_id: 5,
                usn: 101,
                reason: 0x100,
                flags: Flags::NONE,
                name: "later.rs".into(),
            }],
        })
        .unwrap();
    assert_eq!(result.position.next_usn, 110);
    assert_eq!(
        result.table.path_with(result.ids[&9], '\\').as_deref(),
        Some("C:\\later.rs")
    );
}

#[test]
fn invalid_old_or_changed_snapshots_enumerate() {
    for saved in [
        Err(file::FileError::BadChecksum),
        Err(file::FileError::UnsupportedVersion {
            found: 1,
            supported: 2,
        }),
    ] {
        let result = restore_or_enumerate(saved, DRIVE, |reason| {
            assert_eq!(reason, BuildReason::SnapshotInvalid);
            NtfsIndex::restore(snapshot(), 42, 7)
        })
        .unwrap();
        assert_eq!(result.position.next_usn, 100);
    }
    let missing = Err(file::FileError::Io(std::io::ErrorKind::NotFound.into()));
    restore_or_enumerate(missing, DRIVE, |reason| {
        assert_eq!(reason, BuildReason::SnapshotMissing);
        NtfsIndex::restore(snapshot(), 42, 7)
    })
    .unwrap();
    let other_root = restore_or_enumerate(
        Ok(snapshot()),
        Drive {
            letter: 'D',
            ..DRIVE
        },
        |reason| {
            assert_eq!(reason, BuildReason::SnapshotInvalid);
            NtfsIndex::restore(snapshot(), 42, 7)
        },
    );
    assert!(other_root.is_ok());
    let mut enumerated = false;
    restore_or_enumerate(
        Ok(snapshot()),
        Drive {
            serial: 43,
            ..DRIVE
        },
        |reason| {
            assert_eq!(reason, BuildReason::SnapshotStale);
            enumerated = true;
            NtfsIndex::restore(snapshot(), 42, 7)
        },
    )
    .unwrap();
    assert!(enumerated);
}

#[test]
fn save_waits_for_quiet_and_never_saves_a_pending_rename_even_on_stop() {
    let mut index = NtfsIndex::restore(snapshot(), 42, 7).unwrap();
    let mut clock = SaveClock::default();
    clock.changed(Duration::ZERO);
    assert!(!clock.due(Duration::from_secs(29), index.snapshot().as_ref(), false));
    assert!(clock.due(Duration::from_secs(30), index.snapshot().as_ref(), false));
    index
        .apply_batch(UsnBatch {
            next_usn: 110,
            records: vec![UsnRecord {
                size: None,
                file_id: 5,
                parent_id: 5,
                usn: 101,
                reason: 0x1000,
                flags: Flags::FOLDER,
                name: "old".into(),
            }],
        })
        .unwrap();
    assert!(index.snapshot().is_none());
    assert!(!clock.due(Duration::from_secs(60), index.snapshot().as_ref(), true));
    clock.saved();
    assert!(!clock.due(Duration::from_secs(60), Some(&()), true));
}

#[test]
fn failed_saves_back_off_thirty_seconds_then_five_minutes() {
    let mut clock = SaveClock::default();
    let seconds = Duration::from_secs;
    clock.changed(Duration::ZERO);
    clock.failed(seconds(30));
    assert!(!clock.due(seconds(59), Some(&()), false));
    clock.changed(seconds(58));
    assert!(clock.due(seconds(60), Some(&()), false));
    clock.failed(seconds(60));
    assert!(!clock.due(seconds(359), Some(&()), false));
    assert!(clock.due(seconds(360), Some(&()), false));
    assert!(clock.due(seconds(61), Some(&()), true));
    clock.saved();
    clock.changed(seconds(400));
    assert!(clock.due(seconds(430), Some(&()), false));
    clock.failed(seconds(430));
    assert!(clock.due(seconds(460), Some(&()), false));
}

#[test]
fn worker_panics_report_the_payload_and_cancel_the_service() {
    let cancel = navo_index::pipe::CancelToken::new();
    let mut errors = Vec::new();
    let result = super::lifecycle::guard_worker(
        &cancel,
        |error| errors.push(error),
        || panic!("worker fault"),
    );
    assert!(result.is_err());
    assert!(cancel.is_cancelled());
    assert!(errors[0].contains("worker fault"));
}

#[test]
fn a_successful_tick_clears_only_its_own_failures() {
    let mut errors = vec![
        format!("{}access denied", super::lifecycle::UPDATE_FAILURE),
        format!("{}disk full", super::lifecycle::SAVE_FAILURE),
        format!("{}file references", super::lifecycle::UNRESOLVED),
    ];
    super::lifecycle::clear_failures(&mut errors, super::lifecycle::UNRESOLVED);
    super::lifecycle::clear_failures(&mut errors, super::lifecycle::UPDATE_FAILURE);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("disk full"));
    super::lifecycle::clear_failures(&mut errors, super::lifecycle::SAVE_FAILURE);
    assert!(errors.is_empty());
}

#[test]
fn only_an_enabled_navo_service_covers_volumes_and_a_legacy_one_does_not() {
    let removable = Drive {
        letter: 'E',
        fixed: false,
        ..DRIVE
    };
    let discover = || Ok(vec![DRIVE, removable]);
    let only = |wanted: super::install_plan::Product| {
        move |product| Ok((product == wanted).then_some(true))
    };
    let search = Search::default();
    let legacy =
        super::service_volume_roots(only(super::install_plan::LEGACY_PRODUCT), discover, &search);
    assert_eq!(legacy.unwrap(), None);
    let navo = super::service_volume_roots(only(super::install_plan::NAVO), discover, &search);
    assert_eq!(navo.unwrap(), Some(vec![r"C:\".to_owned()]));
    let disabled = super::service_volume_roots(|_| Ok(Some(false)), discover, &search);
    assert_eq!(disabled.unwrap(), None);
}

#[test]
fn unknown_sizes_are_reported_once_per_volume_with_the_count() {
    let mut errors = vec!["unrelated error".to_owned()];
    super::lifecycle::report_unknown_sizes(&mut errors, 7);
    super::lifecycle::report_unknown_sizes(&mut errors, 7);
    assert_eq!(errors, ["unrelated error", "sizes unknown: 7"]);
    super::lifecycle::report_unknown_sizes(&mut errors, 0);
    assert_eq!(errors, ["unrelated error"]);
}

fn header() -> Header {
    Header {
        volume_serial: 42,
        journal_id: 7,
        next_usn: 100,
        root: "C:".into(),
        built_at: 0,
    }
}

fn hinted_index(files: usize, removed: usize) -> NtfsIndex {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let mut ids = FrnMap::with_capacity(files + 1);
    ids.insert(5, root);
    for number in 0..files + removed {
        let id = table
            .push(root, &format!("file-{number:07}.txt"), Flags::NONE)
            .unwrap();
        ids.insert(number as u128 + 100, id);
    }
    for number in 0..removed {
        table.remove(number as u32 + 1);
        ids.remove(&(number as u128 + 100));
    }
    NtfsIndex::restore((header(), table, ids), 42, 7).unwrap()
}

#[test]
fn a_first_build_gets_no_hint() {
    assert_eq!(build_hint(None), SizeHint::default());
}

#[test]
fn a_hint_counts_live_entries_not_dead_slots() {
    let index = hinted_index(1_000, 2_000);
    assert!(index.table.slots() as usize > 2 * index.table.len());
    let hint = build_hint(Some(&index));
    let live = index.table.len();
    assert_eq!(hint.entries, live + live / 16);
    assert_eq!(hint.folders, 1);
}

#[test]
fn a_hint_counts_only_live_folders() {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let mut ids = FrnMap::new();
    ids.insert(5, root);
    let mut deleted = Vec::new();
    for number in 0..9u128 {
        let id = table
            .push(root, &format!("gone-folder-{number}"), Flags::FOLDER)
            .unwrap();
        ids.insert(number + 100, id);
        deleted.push((number + 100, id));
    }
    for number in 0..10u128 {
        let id = table
            .push(root, &format!("file-{number}.txt"), Flags::NONE)
            .unwrap();
        ids.insert(number + 200, id);
    }
    for (reference, id) in deleted {
        table.remove(id);
        ids.remove(&reference);
    }
    let index = NtfsIndex::restore((header(), table, ids), 42, 7).unwrap();
    assert_eq!(build_hint(Some(&index)).folders, 1);
}

#[test]
fn a_hint_ignores_renamed_dead_names() {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let file = table
        .push(root, "old-name-that-is-long.txt", Flags::NONE)
        .unwrap();
    table.rename(file, root, "x.txt").unwrap();
    let ids = [(5u128, root), (100, file)].into();
    let index = NtfsIndex::restore((header(), table, ids), 42, 7).unwrap();
    let hint = build_hint(Some(&index));
    assert_eq!(hint.name_bytes, "C:".len() + 1 + "x.txt".len() + 1);
}

#[test]
fn a_hint_caps_every_total() {
    let hint = hint_from_counts(usize::MAX, usize::MAX, usize::MAX);
    assert_eq!(hint.entries, u32::MAX as usize - 1);
    assert_eq!(hint.folders, u32::MAX as usize - 1);
    assert_eq!(hint.name_bytes, u32::MAX as usize);
}
