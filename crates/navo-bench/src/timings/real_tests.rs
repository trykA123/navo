use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::error::TimingError;
use super::host::{Capture, Host, Launch, Subject};
use super::layout;
use super::real::RealHost;
use crate::fixture;
use crate::guard::ScratchRoot;
use crate::probe::ProbeError;
use crate::source::{ReadOnlySource, SourceError};

fn scratch() -> (tempfile::TempDir, ScratchRoot) {
    let parent = tempfile::tempdir().unwrap();
    let root = ScratchRoot::create(parent.path(), None).unwrap();
    (parent, root)
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut found: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

#[test]
fn the_generator_makes_unique_empty_files_inside_the_root_only() {
    let (parent, root) = scratch();
    layout::build(&root).unwrap();
    let before = names_in(parent.path());
    layout::generate(&root, 120).unwrap();
    let folder = root.path().join(layout::GENERATED);
    let names = names_in(&folder);
    assert_eq!(names.len(), 120);
    assert!(
        names
            .iter()
            .all(|name| fs::metadata(folder.join(name)).unwrap().len() == 0)
    );
    assert_eq!(
        names
            .iter()
            .filter(|name| name.starts_with("sensor"))
            .count(),
        3
    );
    assert_eq!(names_in(parent.path()), before);
    for folder in [
        layout::OUT,
        layout::PROFILES,
        layout::METRICS,
        layout::TRACES,
    ] {
        assert!(root.path().join(folder).is_dir());
    }
}

#[test]
fn generating_twice_is_refused_instead_of_overwriting() {
    let (_parent, root) = scratch();
    layout::build(&root).unwrap();
    layout::generate(&root, 5).unwrap();
    assert!(matches!(
        layout::generate(&root, 5),
        Err(TimingError::Guard(_))
    ));
    assert_eq!(names_in(&root.path().join(layout::GENERATED)).len(), 5);
}

/// The owner's folder reaches the host only as an opened, policy-checked source.
fn host<'a>(root: &'a ScratchRoot, owner: Option<PathBuf>) -> RealHost<'a> {
    let source = owner.map(|folder| ReadOnlySource::open(&folder, root.path()).unwrap());
    RealHost::new(root, PathBuf::from("navo"), source)
}

#[test]
fn an_owner_folder_inside_the_run_root_cannot_become_a_source() {
    let (_parent, root) = scratch();
    layout::build(&root).unwrap();
    let inside = root.path().join(layout::OUT);
    let result = ReadOnlySource::open(&inside, root.path());
    assert_eq!(result.err(), Some(SourceError::ScratchRoot));
}

#[cfg(unix)]
#[test]
fn an_owner_folder_swapped_for_a_link_is_refused_before_anything_starts() {
    let (parent, root) = scratch();
    let owner = tempfile::tempdir_in(parent.path()).unwrap();
    let other = tempfile::tempdir_in(parent.path()).unwrap();
    let mut real = host(&root, Some(owner.path().to_path_buf()));
    fixture::move_dir(owner.path(), &parent.path().join("moved")).unwrap();
    fixture::link_dir(owner.path(), other.path()).unwrap();
    let result = real.run_bench(Subject::Owner, 1);
    assert_eq!(
        result.err(),
        Some(TimingError::Probe(ProbeError::BadReadOnly))
    );
}

#[test]
fn a_missing_owner_folder_is_reported_and_an_outside_one_passes_the_argument_check() {
    let (parent, root) = scratch();
    let folder = tempfile::tempdir_in(parent.path()).unwrap();
    let outside = folder.path().to_path_buf();
    let launch = Launch {
        subject: Subject::Owner,
        run: 1,
        metrics: true,
        trace: true,
    };
    let mut none = host(&root, None);
    none.prepare().unwrap();
    assert_eq!(none.launch(launch).err(), Some(TimingError::NoFolder));
    let mut real = host(&root, Some(outside));
    let bench = real.run_bench(Subject::Owner, 1);
    let started = real.launch(launch);
    if cfg!(windows) {
        assert!(bench.is_err());
    } else {
        assert_eq!(
            bench.err(),
            Some(TimingError::Probe(ProbeError::Unsupported))
        );
        assert_eq!(
            started.err(),
            Some(TimingError::Probe(ProbeError::Unsupported))
        );
    }
}

#[test]
fn output_files_are_read_back_through_the_guard() {
    let (_parent, root) = scratch();
    let mut real = host(&root, None);
    real.prepare().unwrap();
    assert_eq!(real.read(Capture::Metrics, 3), None);
    let mut file = root.create_new(&layout::metrics(3)).unwrap();
    file.write_all(b"{\"first_usable_ms\":1.0}").unwrap();
    drop(file);
    assert_eq!(
        real.read(Capture::Metrics, 3).as_deref(),
        Some("{\"first_usable_ms\":1.0}")
    );
    assert_eq!(real.read(Capture::Trace, 3), None);
}

fn snapshot(dir: &Path, found: &mut Vec<(PathBuf, u64)>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let meta = fs::symlink_metadata(entry.path()).unwrap();
        found.push((entry.path(), meta.len()));
        if meta.is_dir() {
            snapshot(&entry.path(), found);
        }
    }
}

#[test]
fn the_planted_owner_folder_is_never_written_by_the_real_host() {
    let planted = fixture::set().unwrap();
    let (mut before, mut after) = (Vec::new(), Vec::new());
    snapshot(planted.dir.path(), &mut before);
    let (_parent, root) = scratch();
    let mut real = host(&root, Some(planted.dir.path().to_path_buf()));
    real.prepare().unwrap();
    let _ = real.run_bench(Subject::Owner, 1);
    let launch = Launch {
        subject: Subject::Owner,
        run: 2,
        metrics: true,
        trace: true,
    };
    let _ = real.launch(launch);
    snapshot(planted.dir.path(), &mut after);
    assert!(before.len() > 10);
    assert_eq!(before, after);
}
