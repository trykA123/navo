use super::*;

#[test]
fn failure_with_old_file_keeps_old_content_until_replaced() {
    for stage in ALL_STAGES {
        let dir = tmp();
        let target = dir.path().join("settings.toml");
        fs::write(&target, b"old").unwrap();
        let _ = write_atomic_with(&target, b"new", fail_at(stage));
        let want: &[u8] = if stage == Stage::Replaced {
            b"new"
        } else {
            b"old"
        };
        assert_eq!(fs::read(&target).unwrap(), want, "failed at {stage:?}");
    }
}

#[test]
fn failure_without_old_file_leaves_no_target_until_replaced() {
    for stage in [Stage::TempWritten, Stage::TempSynced, Stage::Replaced] {
        let dir = tmp();
        let target = dir.path().join("settings.toml");
        let _ = write_atomic_with(&target, b"new", fail_at(stage));
        let got = fs::read(&target).ok();
        let want = (stage == Stage::Replaced).then(|| b"new".to_vec());
        assert_eq!(got, want, "failed at {stage:?}");
    }
}

#[test]
fn failure_with_old_file_leaves_no_temp_file() {
    for stage in ALL_STAGES {
        let dir = tmp();
        let target = dir.path().join("settings.toml");
        fs::write(&target, b"old").unwrap();
        let _ = write_atomic_with(&target, b"new", fail_at(stage));
        assert_eq!(
            temp_files(dir.path()),
            Vec::<String>::new(),
            "failed at {stage:?}"
        );
    }
}

#[test]
fn failure_without_old_file_leaves_no_temp_file() {
    for stage in [Stage::TempWritten, Stage::TempSynced, Stage::Replaced] {
        let dir = tmp();
        let target = dir.path().join("settings.toml");
        let _ = write_atomic_with(&target, b"new", fail_at(stage));
        assert_eq!(
            temp_files(dir.path()),
            Vec::<String>::new(),
            "failed at {stage:?}"
        );
    }
}

#[test]
fn failure_with_old_file_returns_the_hook_error() {
    for stage in ALL_STAGES {
        let dir = tmp();
        let target = dir.path().join("settings.toml");
        fs::write(&target, b"old").unwrap();
        let got = write_atomic_with(&target, b"new", fail_at(stage)).map_err(|e| e.kind());
        assert_eq!(got, Err(INJECTED), "failed at {stage:?}");
    }
}

#[test]
fn failure_without_old_file_returns_the_hook_error() {
    for stage in [Stage::TempWritten, Stage::TempSynced, Stage::Replaced] {
        let dir = tmp();
        let target = dir.path().join("settings.toml");
        let got = write_atomic_with(&target, b"new", fail_at(stage)).map_err(|e| e.kind());
        assert_eq!(got, Err(INJECTED), "failed at {stage:?}");
    }
}

#[test]
fn backup_stage_is_skipped_without_old_file() {
    let dir = tmp();
    let target = dir.path().join("settings.toml");
    write_atomic_with(&target, b"new", fail_at(Stage::BackupMade)).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"new");
}
