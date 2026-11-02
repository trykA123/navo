use std::sync::Arc;

use rusty_core::{FileStamp, write_plan};
use rusty_fs::ProviderOps;

use super::super::{Drive, drive_retains};
use super::*;

fn stamp() -> FileStamp {
    FileStamp {
        size: 3,
        modified_ms: Some(1_000),
        hash: [0_u8; 32],
    }
}

fn write(fs: &FakeFs, staged_len: u64) -> OperationPlan {
    write_plan(
        &fs.location("dest/a"),
        &fs.location("dest/.a.navo-part"),
        stamp(),
        staged_len,
    )
}

#[test]
fn unreadable_retention_settings_refuse_a_write_plan() {
    let (_root, dirs, journal) = setup();
    std::fs::write(dirs.settings_file(), "[retention]\nretain-deletes = \n").unwrap();
    let fs = FakeFs::new();
    let plan = write(&fs, 11);
    let factory = crate::jobs::experimental_factory(journal.clone(), dirs.clone());
    let Err(message) = factory(&plan) else {
        panic!("a write plan was allowed with a broken navo.toml");
    };
    let file = dirs.settings_file().display().to_string();
    assert!(message.contains(&file), "{message}");
    assert!(crate::jobs::lock(&journal).unfinished().unwrap().is_empty());
}

#[test]
fn a_non_retaining_drive_allows_a_write_plan() {
    let (_root, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("dest/a", 9);
    fs.set_volume_serial("dest", None);
    let plan = write(&fs, 11);
    let ops: Arc<dyn ProviderOps> = Arc::new(fs);
    let factory = crate::jobs::retaining_factory(journal, dirs, ops);
    let log = factory(&plan).unwrap();
    assert!(!log.preserve_overwrites());
}

#[test]
fn a_write_plan_probes_its_target_for_retention() {
    let fs = FakeFs::new();
    fs.add_file("dest/a", 9);
    fs.set_volume_serial("dest", None);
    let plan = write(&fs, 11);
    let ops: Arc<dyn ProviderOps> = Arc::new(fs);
    assert!(!drive_retains(Some(&Drive::Probed(ops)), &plan));

    let fs = FakeFs::new();
    fs.add_file("dest/a", 9);
    fs.set_volume_serial("dest", Some(7));
    let plan = write(&fs, 11);
    let ops: Arc<dyn ProviderOps> = Arc::new(fs);
    assert!(drive_retains(Some(&Drive::Probed(ops)), &plan));
}

#[test]
fn a_retaining_drive_refuses_a_save_until_writes_keep_old_versions() {
    let (_root, dirs, journal) = setup();
    let fs = FakeFs::new();
    fs.add_file("dest/a", 9);
    fs.set_volume_serial("dest", Some(7));
    let plan = write(&fs, 11);
    let ops: Arc<dyn ProviderOps> = Arc::new(fs);
    let factory = crate::jobs::retaining_factory(journal.clone(), dirs, ops);
    let Err(message) = factory(&plan) else {
        panic!("a save ran without keeping the old version");
    };
    assert!(message.contains("not supported yet"), "{message}");
    assert!(crate::jobs::lock(&journal).unfinished().unwrap().is_empty());
}
