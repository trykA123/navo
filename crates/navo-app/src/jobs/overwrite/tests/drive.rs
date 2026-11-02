use std::sync::Arc;

use super::*;

fn overwrite_on(
    kind: OpKind,
    identity: bool,
) -> (Arc<FakeFs>, JobOutcome, StoreDirs, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let dirs = StoreDirs::in_profile(root.path());
    dirs.ensure().unwrap();
    let journal = open(&dirs.journal_file()).unwrap();
    let fs = Arc::new(FakeFs::new());
    fs.add_file("src/a", 5);
    fs.add_file("dest/a", 9);
    if !identity {
        fs.set_volume_serial("dest", None);
    }
    let plan = overwrite(
        fs.as_ref(),
        &[fs.location("src/a")],
        fs.location("dest"),
        kind,
    );
    let ops: Arc<dyn ProviderOps> = fs.clone();
    let mut log = crate::jobs::retaining_factory(journal, dirs.clone(), ops)(&plan).unwrap();
    let outcome = execute(log.as_mut(), &plan, fs.as_ref());
    log.finish(&outcome, fs.as_ref()).unwrap();
    (fs, outcome, dirs, root)
}

#[test]
fn an_overwrite_on_a_drive_that_cannot_retain_replaces_the_file_as_before() {
    for kind in [OpKind::Copy, OpKind::Move] {
        let (fs, outcome, dirs, _root) = overwrite_on(kind, false);
        assert_eq!(outcome, JobOutcome::Done, "{kind:?}");
        assert_eq!(fs.metadata(&fs.location("dest/a")).unwrap().size, 5);
        assert_eq!(
            fs.metadata(&fs.location("src/a")).is_ok(),
            kind == OpKind::Copy
        );
        assert!(entries(&dirs).is_empty());
    }
}

#[test]
fn an_overwrite_on_a_drive_that_retains_keeps_the_old_version() {
    let (fs, outcome, dirs, _root) = overwrite_on(OpKind::Copy, true);
    assert_eq!(outcome, JobOutcome::Done);
    assert_eq!(fs.metadata(&fs.location("dest/a")).unwrap().size, 5);
    assert_eq!(entries(&dirs).len(), 1);
}
