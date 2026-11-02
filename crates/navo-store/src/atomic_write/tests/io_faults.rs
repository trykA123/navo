use super::*;

const FULL: io::ErrorKind = io::ErrorKind::StorageFull;

fn seeded(dir: &TempDir) -> std::path::PathBuf {
    let target = dir.path().join("session.bin");
    fs::write(&target, b"old").unwrap();
    fs::write(backup_path(&target), b"older").unwrap();
    target
}

#[test]
fn a_fault_before_the_replace_keeps_the_old_file() {
    for op in ALL_OPS.into_iter().filter(|op| *op != Op::FolderSync) {
        let dir = tmp();
        let target = seeded(&dir);
        inject(op, FULL);
        let got = write_atomic(&target, b"new").map_err(|e| e.kind());
        assert_eq!(got, Err(FULL), "failed at {op:?}");
        assert_eq!(fs::read(&target).unwrap(), b"old", "failed at {op:?}");
    }
}

#[test]
fn a_fault_at_any_step_leaves_no_temp_file() {
    for op in ALL_OPS {
        let dir = tmp();
        let target = seeded(&dir);
        inject(op, FULL);
        let _ = write_atomic(&target, b"new");
        assert_eq!(temp_files(dir.path()), Vec::<String>::new(), "{op:?}");
    }
}

#[test]
fn a_folder_sync_fault_reports_an_error_after_the_new_file_landed() {
    let dir = tmp();
    let target = seeded(&dir);
    inject(Op::FolderSync, FULL);
    assert_eq!(write_atomic(&target, b"new").unwrap_err().kind(), FULL);
    assert_eq!(fs::read(&target).unwrap(), b"new");
    assert_eq!(fs::read(backup_path(&target)).unwrap(), b"old");
}

#[test]
fn a_folder_sync_fault_is_a_committed_outcome_not_a_failure() {
    let dir = tmp();
    let target = seeded(&dir);
    inject(Op::FolderSync, FULL);
    let outcome = write_checked(&target, b"new", true, || Ok(())).unwrap();
    assert!(matches!(outcome, WriteOutcome::Committed(e) if e.kind() == FULL));
}

#[test]
fn a_backup_fault_is_a_failure_not_a_committed_outcome() {
    for op in BACKUP_OPS {
        let dir = tmp();
        let target = seeded(&dir);
        inject(op, FULL);
        let got = write_checked(&target, b"new", true, || Ok(())).map(|_| ());
        assert_eq!(got.map_err(|e| e.kind()), Err(FULL), "{op:?}");
        assert_eq!(fs::read(&target).unwrap(), b"old", "{op:?}");
    }
}

#[test]
fn a_rename_fault_after_the_backup_keeps_old_content_in_both_files() {
    let dir = tmp();
    let target = seeded(&dir);
    inject(Op::Rename, FULL);
    let _ = write_atomic(&target, b"new");
    assert_eq!(fs::read(&target).unwrap(), b"old");
    assert_eq!(fs::read(backup_path(&target)).unwrap(), b"old");
}

#[test]
fn a_fault_without_an_old_file_leaves_no_target_and_no_temp() {
    for op in [Op::Write, Op::Sync, Op::Rename] {
        let dir = tmp();
        let target = dir.path().join("session.bin");
        inject(op, FULL);
        assert!(write_atomic(&target, b"new").is_err(), "{op:?}");
        assert!(!target.exists(), "{op:?}");
        assert_eq!(temp_files(dir.path()), Vec::<String>::new(), "{op:?}");
    }
}

#[test]
fn backup_faults_do_not_touch_a_write_without_backup() {
    for op in BACKUP_OPS {
        let dir = tmp();
        let target = seeded(&dir);
        inject(op, FULL);
        replace_atomic(&target, b"new").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new", "{op:?}");
        assert_eq!(fs::read(backup_path(&target)).unwrap(), b"older", "{op:?}");
    }
}

#[test]
fn a_disk_full_stream_keeps_the_old_file_and_reports_disk_full() {
    let dir = tmp();
    let target = seeded(&dir);
    let error = write_atomic_streamed(&target, &|out| {
        out.write_all(&[7; 64 * 1024])?;
        Err(io::Error::from(FULL))
    })
    .unwrap_err();
    assert_eq!(error.kind(), FULL);
    assert_eq!(fs::read(&target).unwrap(), b"old");
    assert_eq!(fs::read(backup_path(&target)).unwrap(), b"older");
    assert_eq!(temp_files(dir.path()), Vec::<String>::new());
}

#[test]
fn a_write_after_a_cleared_fault_succeeds() {
    let dir = tmp();
    let target = seeded(&dir);
    inject(Op::Sync, FULL);
    assert!(write_atomic(&target, b"new").is_err());
    clear_fault();
    write_atomic(&target, b"new").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"new");
    assert_eq!(fs::read(backup_path(&target)).unwrap(), b"old");
}

fn crash_at(stage: Stage) -> impl FnMut(Stage) -> io::Result<()> {
    move |reached| {
        assert_ne!(reached, stage, "injected crash");
        Ok(())
    }
}

#[test]
fn a_panic_at_any_stage_leaves_no_temp_and_keeps_the_old_file() {
    for stage in ALL_STAGES.into_iter().filter(|s| *s != Stage::Replaced) {
        let dir = tmp();
        let target = seeded(&dir);
        let crashed =
            std::panic::catch_unwind(|| write_atomic_with(&target, b"new", crash_at(stage)));
        assert!(crashed.is_err(), "{stage:?}");
        assert_eq!(fs::read(&target).unwrap(), b"old", "{stage:?}");
        assert_eq!(temp_files(dir.path()), Vec::<String>::new(), "{stage:?}");
    }
}

#[test]
fn a_panicking_stream_leaves_no_temp() {
    let dir = tmp();
    let target = seeded(&dir);
    let crashed = std::panic::catch_unwind(|| {
        write_atomic_streamed(&target, &|out| {
            out.write_all(b"partial")?;
            panic!("injected crash in the stream")
        })
    });
    assert!(crashed.is_err());
    assert_eq!(fs::read(&target).unwrap(), b"old");
    assert_eq!(temp_files(dir.path()), Vec::<String>::new());
}
