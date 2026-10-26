use std::time::Duration;

use rusty_index::file::{self, Header};
use rusty_index::ntfs::{NtfsError, NtfsIndex};
use rusty_index::table::{Flags, NO_PARENT, NameTable};
use rusty_index::usn::{UsnBatch, UsnRecord};
use rusty_store::Search;

use super::Action;

const DRIVE: Drive = Drive {
    letter: 'C',
    serial: 42,
    fixed: true,
    ntfs: true,
};
use super::config::{Config, Drive, index_path, select_volumes};
use super::lifecycle::{SaveClock, restore_or_enumerate};

#[test]
fn index_service_arguments_stand_alone() {
    for (text, action) in [
        ("run", Action::Run),
        ("install", Action::Install),
        ("remove", Action::Remove),
    ] {
        let args = ["--index-service", text].map(Into::into);
        assert_eq!(
            crate::cli::parse(args).unwrap(),
            crate::cli::Command::IndexService(action)
        );
    }
    for args in [
        vec!["--index-service"],
        vec!["--index-service", "unknown"],
        vec!["--index-service", "run", "extra"],
        vec!["C:\\", "--index-service", "run"],
    ] {
        assert!(crate::cli::parse(args.into_iter().map(Into::into)).is_err());
    }
}

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

fn snapshot() -> (Header, NameTable, rusty_index::usn::FrnMap) {
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
    let result = restore_or_enumerate(
        Ok(snapshot()),
        DRIVE,
        |index| {
            assert_eq!(index.position.next_usn, 100);
            index.apply_batch(UsnBatch {
                next_usn: 110,
                records: vec![UsnRecord {
                    file_id: 9,
                    parent_id: 5,
                    usn: 101,
                    reason: 0x100,
                    flags: Flags::NONE,
                    name: "later.rs".into(),
                }],
            })?;
            Ok(())
        },
        || panic!("a valid snapshot must resume"),
    )
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
        let result = restore_or_enumerate(
            saved,
            DRIVE,
            |_| panic!("invalid file"),
            || NtfsIndex::restore(snapshot(), 42, 7),
        )
        .unwrap();
        assert_eq!(result.position.next_usn, 100);
    }
    for serial in [42, 43] {
        let mut enumerated = false;
        restore_or_enumerate(
            Ok(snapshot()),
            Drive { serial, ..DRIVE },
            |_| Err(NtfsError::Reenumerate),
            || {
                enumerated = true;
                NtfsIndex::restore(snapshot(), 42, 7)
            },
        )
        .unwrap();
        assert!(enumerated);
    }
}

#[test]
fn save_waits_for_quiet_and_never_saves_a_pending_rename_even_on_stop() {
    let mut index = NtfsIndex::restore(snapshot(), 42, 7).unwrap();
    let mut clock = SaveClock::default();
    clock.changed(Duration::ZERO);
    assert!(!clock.due(Duration::from_secs(29), index.snapshot(), false));
    assert!(clock.due(Duration::from_secs(30), index.snapshot(), false));
    index
        .apply_batch(UsnBatch {
            next_usn: 110,
            records: vec![UsnRecord {
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
    assert!(!clock.due(Duration::from_secs(60), index.snapshot(), true));
    clock.saved();
    assert!(!clock.due(Duration::from_secs(60), Some(()), true));
}

#[test]
fn failed_saves_back_off_thirty_seconds_then_five_minutes() {
    let mut clock = SaveClock::default();
    let seconds = Duration::from_secs;
    clock.changed(Duration::ZERO);
    clock.failed(seconds(30));
    assert!(!clock.due(seconds(59), Some(()), false));
    clock.changed(seconds(58));
    assert!(clock.due(seconds(60), Some(()), false));
    clock.failed(seconds(60));
    assert!(!clock.due(seconds(359), Some(()), false));
    assert!(clock.due(seconds(360), Some(()), false));
    assert!(clock.due(seconds(61), Some(()), true));
    clock.saved();
    clock.changed(seconds(400));
    assert!(clock.due(seconds(430), Some(()), false));
    clock.failed(seconds(430));
    assert!(clock.due(seconds(460), Some(()), false));
}

#[test]
fn worker_panics_report_the_payload_and_cancel_the_service() {
    let cancel = rusty_index::pipe::CancelToken::new();
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
