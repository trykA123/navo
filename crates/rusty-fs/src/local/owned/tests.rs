use super::*;
use crate::{CancelToken, LocalFs, ProviderOps};
use rusty_core::FsErrorKind;
use std::cell::Cell;

fn at(path: &std::path::Path) -> rusty_core::Location {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        rusty_core::Location::local(rusty_core::ProviderPath::from_bytes(
            path.as_os_str().as_bytes(),
        ))
        .unwrap()
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        rusty_core::Location::local(rusty_core::ProviderPath::from_wide(
            &path.as_os_str().encode_wide().collect::<Vec<_>>(),
        ))
        .unwrap()
    }
}

#[test]
fn exclusive_creation_is_registered_from_the_open_handle_before_copying_bytes() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("temporary");
    std::fs::write(&source, b"original bytes").unwrap();
    let registered = Cell::new(false);
    let mut created = |identity| {
        #[cfg(not(windows))]
        assert_eq!(std::fs::metadata(&target).unwrap().len(), 0);
        assert_eq!(
            LocalFs::new().file_identity(&at(&target)).unwrap(),
            Some(identity)
        );
        registered.set(true);
        Ok(())
    };
    let mut report = |_| assert!(registered.get());
    let count = copy(
        OwnedCopy {
            source: &at(&source),
            target: &at(&target),
            created: &mut created,
        },
        CopyControl {
            progress: &mut report,
            cancel: &CancelToken::new(),
        },
    )
    .unwrap();
    assert_eq!(count, 14);
    assert_eq!(std::fs::read(target).unwrap(), b"original bytes");
}

#[test]
fn failed_registration_writes_no_bytes_and_preserves_the_created_identity() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("temporary");
    std::fs::write(&source, b"original").unwrap();
    let captured = Cell::new(None);
    let mut created = |identity| {
        captured.set(Some(identity));
        Err(FsError::new(FsErrorKind::Unknown))
    };
    assert!(
        copy(
            OwnedCopy {
                source: &at(&source),
                target: &at(&target),
                created: &mut created
            },
            CopyControl {
                progress: &mut |_| panic!("unregistered bytes"),
                cancel: &CancelToken::new()
            }
        )
        .is_err()
    );
    assert!(captured.get().is_some());
    #[cfg(not(windows))]
    {
        assert_eq!(std::fs::metadata(&target).unwrap().len(), 0);
        LocalFs::new()
            .remove_owned_file(&at(&target), captured.get().unwrap())
            .unwrap();
    }
    assert!(!target.exists());
}

#[test]
fn an_existing_temporary_is_never_truncated_or_registered() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("temporary");
    std::fs::write(&source, b"source").unwrap();
    std::fs::write(&target, b"keep collision").unwrap();
    assert!(
        copy(
            OwnedCopy {
                source: &at(&source),
                target: &at(&target),
                created: &mut |_| panic!("existing path registered")
            },
            CopyControl {
                progress: &mut |_| {},
                cancel: &CancelToken::new()
            }
        )
        .is_err()
    );
    assert_eq!(std::fs::read(target).unwrap(), b"keep collision");
}

#[test]
fn a_replacement_at_the_temporary_name_is_preserved_during_cleanup() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("temporary");
    #[cfg(not(windows))]
    let moved = root.path().join("owned");
    std::fs::write(&source, b"original").unwrap();
    let captured = Cell::new(None);
    let mut created = |identity| {
        captured.set(Some(identity));
        Err(FsError::new(FsErrorKind::Unknown))
    };
    assert!(
        copy(
            OwnedCopy {
                source: &at(&source),
                target: &at(&target),
                created: &mut created
            },
            CopyControl {
                progress: &mut |_| {},
                cancel: &CancelToken::new()
            }
        )
        .is_err()
    );
    #[cfg(not(windows))]
    std::fs::rename(&target, &moved).unwrap();
    assert!(!target.exists());
    std::fs::write(&target, b"replacement bytes").unwrap();
    assert!(
        LocalFs::new()
            .remove_owned_file(&at(&target), captured.get().unwrap())
            .is_err()
    );
    assert_eq!(std::fs::read(target).unwrap(), b"replacement bytes");
    #[cfg(not(windows))]
    assert_eq!(std::fs::metadata(moved).unwrap().len(), 0);
}

#[test]
fn cancellation_leaves_no_partial_or_only_a_proved_one_for_the_owner() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("temporary");
    std::fs::write(&source, vec![7; 3 << 20]).unwrap();
    let captured = Cell::new(None);
    let cancel = CancelToken::new();
    let mut created = |identity| {
        captured.set(Some(identity));
        Ok(())
    };
    let result = copy(
        OwnedCopy {
            source: &at(&source),
            target: &at(&target),
            created: &mut created,
        },
        CopyControl {
            progress: &mut |_| cancel.cancel(),
            cancel: &cancel,
        },
    );
    assert_eq!(result.unwrap_err().kind, FsErrorKind::Cancelled);
    assert!(captured.get().is_some());
    #[cfg(not(windows))]
    {
        assert!(std::fs::metadata(&target).unwrap().len() < (3 << 20));
        LocalFs::new()
            .remove_owned_file(&at(&target), captured.get().unwrap())
            .unwrap();
    }
    assert!(!target.exists());
}

#[test]
fn copied_readonly_metadata_does_not_prevent_owned_temporary_cleanup() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("temporary");
    std::fs::write(&source, b"readonly bytes").unwrap();
    let original = std::fs::metadata(&source).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    std::fs::set_permissions(&source, readonly).unwrap();
    let captured = Cell::new(None);
    copy(
        OwnedCopy {
            source: &at(&source),
            target: &at(&target),
            created: &mut |identity| {
                captured.set(Some(identity));
                Ok(())
            },
        },
        CopyControl {
            progress: &mut |_| {},
            cancel: &CancelToken::new(),
        },
    )
    .unwrap();
    assert!(std::fs::metadata(&target).unwrap().permissions().readonly());
    LocalFs::new()
        .remove_owned_file(&at(&target), captured.get().unwrap())
        .unwrap();
    assert!(!target.exists());
    std::fs::set_permissions(source, original).unwrap();
}

#[cfg(windows)]
#[test]
fn owned_copy_preserves_alternate_streams_before_source_removal() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("temporary");
    std::fs::write(&source, b"main data").unwrap();
    std::fs::write(root.path().join("source:details"), b"alternate data").unwrap();
    let registered = Cell::new(false);
    let count = copy(
        OwnedCopy {
            source: &at(&source),
            target: &at(&target),
            created: &mut |_| {
                registered.set(true);
                Ok(())
            },
        },
        CopyControl {
            progress: &mut |_| assert!(registered.get()),
            cancel: &CancelToken::new(),
        },
    )
    .unwrap();
    assert_eq!(count, 9);
    assert_eq!(std::fs::read(&target).unwrap(), b"main data");
    assert_eq!(
        std::fs::read(root.path().join("temporary:details")).unwrap(),
        b"alternate data"
    );
    std::fs::remove_file(source).unwrap();
    assert_eq!(
        std::fs::read(root.path().join("temporary:details")).unwrap(),
        b"alternate data"
    );
}
