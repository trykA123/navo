use navo_index::pipe::VolumeStatus;

use super::*;

fn root() -> Location {
    Location::new(r"C:\").unwrap()
}

fn status(entries: u64, errors: Vec<String>) -> Status {
    Status {
        volumes: vec![VolumeStatus {
            root: "C:".into(),
            entries,
            errors,
            ..Default::default()
        }],
    }
}

fn snapshot(folder: &Path, root: &Location) -> PathBuf {
    let path = folder.join(file_name(root));
    if !path.exists() {
        std::fs::write(&path, b"walk").unwrap();
    }
    path
}

fn start(folder: &Path, configured: &[Location], service: Option<&Status>) {
    Snapshots::new(Some(folder.to_owned())).sweep(&Sweep {
        configured,
        listed: true,
        in_use: &[],
        service,
    });
}

#[test]
fn a_covered_snapshot_goes_only_after_three_healthy_starts() {
    let dir = tempfile::tempdir().unwrap();
    let path = snapshot(dir.path(), &root());
    let healthy = status(10, Vec::new());
    for _ in 0..2 {
        start(dir.path(), &[root()], Some(&healthy));
        assert!(path.exists());
    }
    start(dir.path(), &[root()], Some(&healthy));
    assert!(!path.exists());
}

#[test]
fn an_erroring_or_empty_service_resets_the_streak() {
    let dir = tempfile::tempdir().unwrap();
    let path = snapshot(dir.path(), &root());
    let healthy = status(10, Vec::new());
    start(dir.path(), &[root()], Some(&healthy));
    start(dir.path(), &[root()], Some(&healthy));
    start(
        dir.path(),
        &[root()],
        Some(&status(10, vec!["refused".into()])),
    );
    start(dir.path(), &[root()], Some(&status(0, Vec::new())));
    start(dir.path(), &[root()], Some(&healthy));
    start(dir.path(), &[root()], Some(&healthy));
    assert!(path.exists());
}

#[test]
fn a_start_without_the_service_never_removes_a_covered_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = snapshot(dir.path(), &root());
    for _ in 0..5 {
        start(dir.path(), &[root()], None);
    }
    assert!(path.exists());
}

#[test]
fn a_snapshot_of_a_root_no_longer_configured_is_removed() {
    let dir = tempfile::tempdir().unwrap();
    let home = Location::new(r"C:\Users\admin").unwrap();
    let old = snapshot(dir.path(), &home);
    let kept = snapshot(dir.path(), &root());
    let other = dir.path().join("notes.txt");
    std::fs::write(&other, b"x").unwrap();
    start(dir.path(), &[root()], None);
    assert!(!old.exists());
    assert!(kept.exists() && other.exists());
}

#[test]
fn a_snapshot_still_in_use_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let home = Location::new(r"C:\Users\admin").unwrap();
    let path = snapshot(dir.path(), &home);
    let mut snapshots = Snapshots::new(Some(dir.path().to_owned()));
    snapshots.sweep(&Sweep {
        configured: &[],
        listed: true,
        in_use: std::slice::from_ref(&home),
        service: None,
    });
    assert!(path.exists());
}

#[test]
fn each_start_counts_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = snapshot(dir.path(), &root());
    let healthy = status(10, Vec::new());
    let mut snapshots = Snapshots::new(Some(dir.path().to_owned()));
    for _ in 0..5 {
        snapshots.sweep(&Sweep {
            configured: &[root()],
            listed: true,
            in_use: &[],
            service: Some(&healthy),
        });
    }
    assert!(path.exists());
}

#[test]
fn a_snapshot_of_an_absent_drive_is_kept_unless_the_settings_drop_it() {
    let dir = tempfile::tempdir().unwrap();
    let stick = Location::new(r"E:\").unwrap();
    let path = snapshot(dir.path(), &stick);
    let sweep = |listed| {
        Snapshots::new(Some(dir.path().to_owned())).sweep(&Sweep {
            configured: &[root()],
            listed,
            in_use: &[],
            service: None,
        });
    };
    sweep(false);
    assert!(path.exists());
    sweep(true);
    assert!(!path.exists());
}

#[test]
fn a_start_without_a_service_reply_breaks_the_healthy_run() {
    let dir = tempfile::tempdir().unwrap();
    let path = snapshot(dir.path(), &root());
    let healthy = status(10, Vec::new());
    start(dir.path(), &[root()], Some(&healthy));
    start(dir.path(), &[root()], None);
    start(dir.path(), &[root()], Some(&healthy));
    start(dir.path(), &[root()], Some(&healthy));
    assert!(path.exists());
    start(dir.path(), &[root()], Some(&healthy));
    assert!(!path.exists());
}
