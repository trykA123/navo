//! Operation faults prove strict replacement ordering and error propagation.
use super::*;
use std::cell::{Cell, RefCell};

#[test]
fn durable_replace_propagates_faults_without_fallback() {
    let (mut actual, mut expected) = (Vec::new(), Vec::new());
    for (at, code) in [(1, 5), (1, 32), (1, 17), (2, 5), (2, 32), (3, 1117), (0, 0)] {
        for persistent in [false, true] {
            let calls = RefCell::new(Vec::new());
            let failed = Cell::new(false);
            let operation = |step| {
                calls.borrow_mut().push(step);
                let fault = step == at && (!failed.replace(true) || persistent);
                (!fault)
                    .then_some(())
                    .ok_or_else(|| io::Error::from_raw_os_error(code))
            };
            let result = replace_with(operation(1), || operation(2), |()| operation(3));
            let end = if at == 0 { 3 } else { at };
            actual.push((
                result.err().and_then(|e| e.raw_os_error()),
                calls.into_inner(),
            ));
            expected.push(((at != 0).then_some(code), (1..=end).collect::<Vec<_>>()));
        }
    }
    assert_eq!(
        actual, expected,
        "propagate errors, never retry, flush before Ok"
    );
}

#[cfg(windows)]
#[test]
fn windows_durable_replace_succeeds_while_target_has_an_open_reader_handle() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("open-target.bin");
    let source = dir.path().join(".open-target.tmp");
    fs::write(&target, b"old-bytes").expect("write target");
    fs::write(&source, b"new-bytes").expect("write source");

    let reader = fs::File::open(&target).expect("hold shared reader handle");
    native_replace_durable(&source, &target).expect("replace while reader handle is open");
    drop(reader);

    assert_eq!(fs::read(&target).expect("read replaced"), b"new-bytes");
    assert!(!source.exists());
}

fn access_denied_error() -> io::Error {
    #[cfg(windows)]
    {
        io::Error::from_raw_os_error(5)
    }
    #[cfg(not(windows))]
    {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Access is denied. (os error 5)",
        )
    }
}

#[test]
fn user_durable_replace_proceeds_when_barrier_is_permission_denied() {
    let calls = RefCell::new(Vec::new());
    let result = replace_user_with(
        {
            calls.borrow_mut().push(1);
            Err::<(), _>(access_denied_error())
        },
        || {
            calls.borrow_mut().push(2);
            Ok(())
        },
        |()| {
            calls.borrow_mut().push(3);
            Ok(())
        },
    );
    assert!(
        result.is_ok(),
        "barrier PermissionDenied must proceed to replace: {result:?}"
    );
    assert_eq!(
        calls.into_inner(),
        vec![1, 2],
        "volume flush must be skipped when barrier failed with PermissionDenied"
    );
}

#[test]
fn user_durable_replace_propagates_replace_permission_denied_after_barrier_denied() {
    let calls = RefCell::new(Vec::new());
    let result = replace_user_with(
        {
            calls.borrow_mut().push(1);
            Err::<(), _>(access_denied_error())
        },
        || {
            calls.borrow_mut().push(2);
            Err(access_denied_error())
        },
        |()| {
            calls.borrow_mut().push(3);
            Ok(())
        },
    );
    let error = result.expect_err("target ACL PermissionDenied during replace must propagate");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    #[cfg(windows)]
    assert_eq!(error.raw_os_error(), Some(5));
    assert_eq!(
        calls.into_inner(),
        vec![1, 2],
        "replace runs after barrier PermissionDenied and stops before flush"
    );
}

#[test]
fn user_durable_replace_stops_before_replace_on_non_permission_barrier_faults() {
    for code in [32, 17] {
        let calls = RefCell::new(Vec::new());
        let result = replace_user_with(
            {
                calls.borrow_mut().push(1);
                Err::<(), _>(io::Error::from_raw_os_error(code))
            },
            || {
                calls.borrow_mut().push(2);
                Ok(())
            },
            |()| {
                calls.borrow_mut().push(3);
                Ok(())
            },
        );
        let error = result.expect_err("non-PermissionDenied barrier fault must propagate");
        assert_eq!(error.raw_os_error(), Some(code));
        assert_eq!(
            calls.into_inner(),
            vec![1],
            "non-PermissionDenied barrier fault {code} must stop before replace"
        );
    }
}

#[test]
fn user_durable_replace_propagates_flush_faults_after_barrier_succeeds() {
    for (fault, expected_code) in [
        (io::Error::from_raw_os_error(1117), Some(1117)),
        (access_denied_error(), None),
    ] {
        let expected_kind = fault.kind();
        let calls = RefCell::new(Vec::new());
        let result = replace_user_with(
            {
                calls.borrow_mut().push(1);
                Ok(())
            },
            || {
                calls.borrow_mut().push(2);
                Ok(())
            },
            |()| {
                calls.borrow_mut().push(3);
                Err(fault)
            },
        );
        let error = result.expect_err("flush fault after Ok(barrier) must propagate");
        assert_eq!(error.kind(), expected_kind);
        if let Some(code) = expected_code {
            assert_eq!(error.raw_os_error(), Some(code));
        }
        assert_eq!(calls.into_inner(), vec![1, 2, 3]);
    }
}

#[test]
fn user_durable_replace_replaces_sibling_and_rejects_same_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("target.bin");
    let source = dir.path().join(".target.tmp");
    fs::write(&target, b"old").expect("write target");
    fs::write(&source, b"new").expect("write source");

    let same = native_replace_user_durable(&target, &target).expect_err("same path rejected");
    assert_eq!(same.kind(), io::ErrorKind::InvalidInput);

    native_replace_user_durable(&source, &target).expect("replace sibling");
    assert_eq!(fs::read(&target).expect("read replaced"), b"new");
    assert!(!source.exists());
}

#[cfg(windows)]
#[test]
fn windows_user_durable_replace_succeeds_while_target_has_an_open_reader_handle() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("open-user-target.bin");
    let source = dir.path().join(".open-user-target.tmp");
    fs::write(&target, b"old-bytes").expect("write target");
    fs::write(&source, b"new-bytes").expect("write source");

    let reader = fs::File::open(&target).expect("hold shared reader handle");
    native_replace_user_durable(&source, &target).expect("replace while reader handle is open");
    drop(reader);

    assert_eq!(fs::read(&target).expect("read replaced"), b"new-bytes");
    assert!(!source.exists());
}
