use super::*;

mod regressions;

#[test]
fn a_job_copies_a_file_out_of_a_commit() {
    let _guard = serial();
    let Some(fixture) = fixture() else {
        return;
    };
    fixture.init();
    fixture.write("file.bin", &[0, 1, 2, 255]);
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("file");
    let git = Arc::new(GitFs::new());
    let source = at(&fixture, "HEAD", "file.bin");
    let out = tempfile::tempdir().unwrap();
    let target = local_of(&out.path().join("file.bin"));
    let plan = rusty_core::OperationPlan {
        kind: rusty_core::OpKind::Copy,
        steps: vec![rusty_core::Step::CopyFile {
            size: 4,
            source,
            target,
        }],
        totals: rusty_core::Totals {
            files: 1,
            folders: 0,
            bytes: 4,
        },
        conflicts: Vec::new(),
        risks: Vec::new(),
    };
    let outcome = crate::run_job_with_sources(
        &plan,
        &[],
        &mut crate::JobContext {
            ops: &crate::LocalFs::new(),
            log: &mut Steps,
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
        &crate::Sources::new().with(git),
    );
    assert_eq!(outcome, crate::JobOutcome::Done);
    assert_eq!(
        fs::read(out.path().join("file.bin")).unwrap(),
        [0, 1, 2, 255]
    );
}

#[test]
fn a_job_copies_a_git_file_with_crlf_conversion() {
    let _guard = serial();
    let fixture = fixture().expect("Git is required for the J1 conversion regression");
    fixture.init();
    fixture.write(".gitattributes", b"file.txt text eol=crlf\n");
    fixture.write("file.txt", b"a\nb\n");
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("file with conversion");
    assert_copies_worktree_bytes(&fixture, 4, b"a\r\nb\r\n");
}

#[test]
fn a_job_copies_a_git_file_with_crlf_shrinking_through_a_filter() {
    let _guard = serial();
    let fixture = fixture().expect("Git is required for the J1 conversion regression");
    fixture.init();
    fixture.git_ok(&["config", "filter.lf.smudge", "git stripspace"]);
    fixture.write(".gitattributes", b"file.txt -text filter=lf\n");
    fixture.write("file.txt", b"a\r\nb\r\n");
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("file with shrinking conversion");
    assert_copies_worktree_bytes(&fixture, 6, b"a\nb\n");
}

fn assert_copies_worktree_bytes(fixture: &Fixture, blob_size: u64, expected: &[u8]) {
    let git = Arc::new(GitFs::new());
    let source = at(fixture, "HEAD", "file.txt");
    let size = Provider::metadata(git.as_ref(), &source).unwrap().size;
    let mut read_bytes = Vec::new();
    git.read(&source)
        .unwrap()
        .read_to_end(&mut read_bytes)
        .unwrap();
    assert_eq!(size, blob_size);
    assert_eq!(read_bytes, expected);
    let sources = crate::Sources::new().with(git);
    for workers in [0, 1, 4] {
        let out = tempfile::tempdir().unwrap();
        let plan = conversion_plan(source.clone(), local_of(&out.path().join("file.txt")), size);
        let mut progress = Vec::new();
        let outcome = crate::run_job_parallel_with_sources(
            (&plan, &sources),
            &[],
            &mut crate::JobContext {
                ops: &crate::LocalFs::new(),
                log: &mut Steps,
                cancel: &CancelToken::new(),
                progress: &mut |update| progress.push(update.clone()),
            },
            workers,
        );
        assert_eq!(outcome, crate::JobOutcome::Done);
        assert_eq!(fs::read(out.path().join("file.txt")).unwrap(), read_bytes);
        let last = progress.last().unwrap();
        assert_eq!(last.bytes_done, read_bytes.len() as u64);
        assert_eq!(last.bytes_total, read_bytes.len() as u64);
        assert_eq!(last.files_done, 1);
        assert!(progress.iter().all(|p| p.bytes_done <= p.bytes_total));
    }
}

fn conversion_plan(source: Location, target: Location, size: u64) -> rusty_core::OperationPlan {
    rusty_core::OperationPlan {
        kind: rusty_core::OpKind::Copy,
        steps: vec![rusty_core::Step::CopyFile {
            size,
            source,
            target,
        }],
        totals: rusty_core::Totals {
            files: 1,
            folders: 0,
            bytes: size,
        },
        conflicts: Vec::new(),
        risks: Vec::new(),
    }
}

#[cfg(unix)]
fn local_of(path: &Path) -> Location {
    use std::os::unix::ffi::OsStrExt;
    Location::local(ProviderPath::from_bytes(path.as_os_str().as_bytes())).unwrap()
}

#[cfg(windows)]
fn local_of(path: &Path) -> Location {
    use std::os::windows::ffi::OsStrExt;
    let units: Vec<u16> = path.as_os_str().encode_wide().collect();
    Location::local(ProviderPath::from_wide(&units)).unwrap()
}

struct Steps;

impl crate::JobLog for Steps {
    fn step(&mut self, _: u32, _: crate::StepOutcome) -> Result<(), FsError> {
        Ok(())
    }
    fn undone(&mut self, _: u32, _: crate::StepOutcome) -> Result<(), FsError> {
        Ok(())
    }
    fn journal_held(&mut self, _: &crate::HeldFile) -> Result<bool, FsError> {
        Ok(true)
    }
}
