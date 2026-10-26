use std::fs;

use rusty_core::{FsErrorKind, Location};
use rusty_fs::{FakeFs, LocalFs, ProviderOps};

#[derive(Default)]
struct RenameLog(Option<rusty_fs::FileIdentity>);

impl rusty_fs::JobLog for RenameLog {
    fn prepare_rename(
        &mut self,
        _: u32,
        identity: rusty_fs::FileIdentity,
    ) -> Result<(), rusty_core::FsError> {
        self.0 = Some(identity);
        Ok(())
    }

    fn step(&mut self, _: u32, _: rusty_fs::StepOutcome) -> Result<(), rusty_core::FsError> {
        Ok(())
    }
    fn undone(&mut self, _: u32, _: rusty_fs::StepOutcome) -> Result<(), rusty_core::FsError> {
        Ok(())
    }
    fn journal_held(&mut self, _: &rusty_fs::HeldFile) -> Result<bool, rusty_core::FsError> {
        Ok(false)
    }
}

#[test]
fn parallel_rename_records_its_identity_before_changing_the_name() {
    use rusty_core::{OpKind, OperationPlan, Step, Totals};
    use rusty_fs::{CancelToken, JobContext, JobOutcome, run_job_parallel};

    let fs = FakeFs::new();
    fs.add_file("source", 5);
    let source = fs.location("source");
    let expected = fs.file_identity(&source).unwrap();
    let plan = OperationPlan {
        kind: OpKind::Rename,
        steps: vec![Step::Rename {
            source,
            target: fs.location("target"),
        }],
        totals: Totals {
            files: 1,
            folders: 0,
            bytes: 0,
        },
        conflicts: Vec::new(),
        risks: Vec::new(),
    };
    let mut log = RenameLog::default();
    let outcome = run_job_parallel(
        &plan,
        &[],
        &mut JobContext {
            ops: &fs,
            log: &mut log,
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
        4,
    );
    assert_eq!(outcome, JobOutcome::Done);
    assert_eq!(log.0, expected);
}

#[expect(
    clippy::unwrap_used,
    reason = "test fixtures use valid UTF-8 temporary paths"
)]
fn location(path: &std::path::Path) -> Location {
    Location::new(path.to_str().unwrap()).unwrap()
}

#[test]
fn local_identity_survives_a_rename_and_an_in_place_edit() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let target = dir.path().join("target");
    fs::write(&source, b"first").unwrap();
    let ops = LocalFs::new();
    let identity = ops.file_identity(&location(&source)).unwrap().unwrap();
    ops.rename(&location(&source), &location(&target)).unwrap();
    fs::write(&target, b"changed in place").unwrap();
    assert_eq!(
        ops.file_identity(&location(&target)).unwrap(),
        Some(identity)
    );
}

#[test]
fn local_identity_distinguishes_replacements_with_matching_size_and_time() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("original");
    let replacement = dir.path().join("replacement");
    fs::write(&original, b"first").unwrap();
    fs::write(&replacement, b"other").unwrap();
    let modified = fs::metadata(&original).unwrap().modified().unwrap();
    fs::File::options()
        .write(true)
        .open(&replacement)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    let ops = LocalFs::new();
    let identity = ops.file_identity(&location(&original)).unwrap().unwrap();
    ops.replace(&location(&replacement), &location(&original))
        .unwrap();
    assert_ne!(
        ops.file_identity(&location(&original)).unwrap(),
        Some(identity)
    );
}

#[test]
fn local_folder_identity_survives_a_rename() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let target = dir.path().join("target");
    fs::create_dir(&source).unwrap();
    let ops = LocalFs::new();
    let identity = ops.file_identity(&location(&source)).unwrap().unwrap();
    ops.rename(&location(&source), &location(&target)).unwrap();
    assert_eq!(
        ops.file_identity(&location(&target)).unwrap(),
        Some(identity)
    );
}

#[test]
fn local_missing_identity_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let error = LocalFs::new()
        .file_identity(&location(&dir.path().join("missing")))
        .unwrap_err();
    assert_eq!(error.kind, FsErrorKind::NotFound);
}

#[cfg(unix)]
#[test]
fn local_link_identity_does_not_follow_its_target() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target");
    let link = dir.path().join("link");
    fs::write(&target, b"target").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let ops = LocalFs::new();
    let identity = ops.file_identity(&location(&link)).unwrap().unwrap();
    assert_ne!(
        ops.file_identity(&location(&target)).unwrap(),
        Some(identity)
    );
    fs::remove_file(&target).unwrap();
    assert_eq!(ops.file_identity(&location(&link)).unwrap(), Some(identity));
}

#[test]
fn fake_identity_survives_renames_and_changes_after_replacement() {
    let fs = FakeFs::new();
    fs.add_file("source", 5);
    let source = fs.location("source");
    let target = fs.location("target");
    let identity = fs.file_identity(&source).unwrap().unwrap();
    fs.rename(&source, &target).unwrap();
    assert_eq!(fs.file_identity(&target).unwrap(), Some(identity));
    fs.remove("target");
    fs.add_file("target", 5);
    assert_ne!(fs.file_identity(&target).unwrap(), Some(identity));
}
