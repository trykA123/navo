use super::*;
use crate::{FakeFs, FakeOp, Fault, Provider};

#[test]
fn mount_query_failure_removes_only_the_new_verified_empty_holder() {
    let fs = FakeFs::new();
    fs.add_file("dest/a", 2);
    fs.add_file("dest/new", 7);
    let aside = Aside::new();
    let held = aside.held_file(&fs, &fs.location("dest/a"), None).unwrap();
    let container = held.held.parent().unwrap();
    fs.fault(
        &container.path().to_text(),
        FakeOp::Mount,
        1,
        Fault::Fail(FsErrorKind::Timeout),
    );
    let original = fs.file_identity(&held.original).unwrap();
    assert!(
        aside
            .prepare_proof(&fs, (&held, &fs.location("dest/new"), None))
            .is_err()
    );
    assert_eq!(fs.file_identity(&held.original).unwrap(), original);
    assert_eq!(
        fs.metadata(&container).unwrap_err().kind,
        FsErrorKind::NotFound
    );
}

#[test]
fn unsupported_replacement_volume_creates_no_holder() {
    let fs = FakeFs::new();
    fs.add_file("dest/a", 2);
    fs.add_file("dest/new", 7);
    fs.set_mount_identity("dest/new", Some(99));
    let aside = Aside::new();
    let held = aside.held_file(&fs, &fs.location("dest/a"), None).unwrap();
    assert!(
        aside
            .prepare_proof(&fs, (&held, &fs.location("dest/new"), None))
            .is_err()
    );
    assert_eq!(
        fs.metadata(&held.held.parent().unwrap()).unwrap_err().kind,
        FsErrorKind::NotFound
    );
}
