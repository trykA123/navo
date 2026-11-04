use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(not(windows))]
use super::host::{Exe, Spec};
use super::host::{Host, MeasureError};
use super::layout::{self, A_FILES, B_FILES_PER_FOLDER, B_FOLDERS};
use super::names::Names;
use super::real::RealHost;
use crate::fixture;
use crate::guard::ScratchRoot;

fn files_in(dir: &Path) -> Vec<String> {
    let mut found: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

fn scratch() -> (tempfile::TempDir, ScratchRoot) {
    let parent = tempfile::tempdir().unwrap();
    let root = ScratchRoot::create(parent.path(), None).unwrap();
    (parent, root)
}

fn tester(dir: &tempfile::TempDir) -> PathBuf {
    fixture::stand_in_exe(dir.path(), 4096, true).unwrap();
    dir.path()
        .join(if cfg!(windows) { "navo.exe" } else { "navo" })
}

#[test]
fn names_repeat_for_a_seed_and_never_collide() {
    let first: Vec<_> = {
        let mut names = Names::new(7);
        (0..200).map(|index| names.file(index)).collect()
    };
    let mut again = Names::new(7);
    let second: Vec<_> = (0..200).map(|index| again.file(index)).collect();
    assert_eq!(first, second);
    let mut unique = first.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 200);
    assert!(first[0].starts_with("sensor") && first[50].starts_with("sensor"));
    assert!(!first[1].starts_with("sensor"));
    assert!(
        first
            .iter()
            .all(|name| name.contains('.') && name.len() < 24)
    );
}

#[test]
fn fixtures_a_and_b_have_the_planned_shape_and_stay_inside_the_root() {
    let (_parent, root) = scratch();
    layout::build(&root).unwrap();
    let a = root.path().join(layout::FIXTURE_A);
    assert_eq!(files_in(&a).len(), A_FILES);
    let b = root.path().join(layout::FIXTURE_B);
    let folders: Vec<_> = fs::read_dir(&b)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(folders.len(), B_FOLDERS);
    assert!(
        folders
            .iter()
            .all(|f| files_in(f).len() == B_FILES_PER_FOLDER)
    );
    for folder in [layout::PROFILES, layout::METRICS, layout::EXES] {
        assert!(root.path().join(folder).is_dir());
    }
}

#[test]
fn building_twice_is_refused_instead_of_overwriting() {
    let (_parent, root) = scratch();
    layout::build(&root).unwrap();
    assert!(matches!(layout::build(&root), Err(MeasureError::Guard(_))));
}

#[test]
fn a_fresh_copy_is_made_through_the_guard_and_matches_the_original() {
    let (parent, root) = scratch();
    let exe = tester(&parent);
    let mut host = RealHost::new(&root, exe.clone());
    host.prepare().unwrap();
    host.copy_exe(1).unwrap();
    host.copy_exe(2).unwrap();
    let copy = |n| {
        root.path()
            .join(layout::exe_folder(n))
            .join(exe.file_name().unwrap())
    };
    assert_eq!(fs::read(copy(1)).unwrap(), fs::read(&exe).unwrap());
    assert_eq!(fs::read(copy(2)).unwrap(), fs::read(&exe).unwrap());
    assert!(matches!(host.copy_exe(2), Err(MeasureError::Guard(_))));
    assert_eq!(host.exe_size(), Ok(fs::metadata(&exe).unwrap().len()));
}

#[test]
fn metrics_are_read_from_the_run_root_only_once_they_exist() {
    let (parent, root) = scratch();
    let mut host = RealHost::new(&root, tester(&parent));
    host.prepare().unwrap();
    assert_eq!(host.read_metrics(7), None);
    let mut file = root.create_new(&layout::metrics(7)).unwrap();
    file.write_all(b"{\"first_usable_ms\": 5}").unwrap();
    drop(file);
    assert_eq!(
        host.read_metrics(7).as_deref(),
        Some("{\"first_usable_ms\": 5}")
    );
}

#[test]
fn the_clock_pauses_for_real_and_a_missing_exe_has_no_size() {
    let (parent, root) = scratch();
    let mut host = RealHost::new(&root, parent.path().join("absent"));
    let before = host.elapsed();
    host.pause(Duration::from_millis(5));
    assert!(host.elapsed() >= before + Duration::from_millis(5));
    assert!(matches!(host.exe_size(), Err(MeasureError::Io(_))));
}

#[cfg(not(windows))]
#[test]
fn off_windows_a_launch_is_refused_after_the_guard_checks() {
    let (parent, root) = scratch();
    let mut host = RealHost::new(&root, tester(&parent));
    host.prepare().unwrap();
    let spec = Spec {
        exe: Exe::Original,
        run: 1,
        metrics: true,
    };
    let error = host.launch(spec).err().unwrap();
    assert_eq!(
        error,
        MeasureError::Probe(crate::probe::ProbeError::Unsupported)
    );
    assert!(root.path().join(layout::profile(1)).is_dir());
}
