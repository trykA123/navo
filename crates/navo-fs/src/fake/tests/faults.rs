use super::*;

fn metadata_kind(fs: &FakeFs, path: &str) -> Option<FsErrorKind> {
    kind_of(fs.metadata(&fs.location(path)))
}

#[test]
fn fail_on_the_first_call() {
    let fs = with_files(1);
    fs.fault("d", FakeOp::List, 1, Fault::Fail(FsErrorKind::AccessDenied));
    assert_eq!(
        kind_of(list_all(&fs, "d").0),
        Some(FsErrorKind::AccessDenied)
    );
}

#[test]
fn fail_sends_no_batch() {
    let fs = with_files(1);
    fs.fault("d", FakeOp::List, 1, Fault::Fail(FsErrorKind::AccessDenied));
    assert_eq!(list_all(&fs, "d").1, []);
}

#[test]
fn fault_error_carries_the_location() {
    let fs = with_files(1);
    fs.fault("d", FakeOp::List, 1, Fault::Fail(FsErrorKind::Locked));
    assert_eq!(
        list_all(&fs, "d").0.err().and_then(|e| e.location),
        Some(fs.location("d"))
    );
}

#[test]
fn fail_on_the_third_call_only() {
    let fs = with_files(1);
    fs.fault(
        "d/f00",
        FakeOp::Metadata,
        3,
        Fault::Fail(FsErrorKind::DiskFull),
    );
    let kinds: Vec<_> = (0..4).map(|_| metadata_kind(&fs, "d/f00")).collect();
    assert_eq!(kinds, [None, None, Some(FsErrorKind::DiskFull), None]);
}

#[test]
fn counter_counts_calls() {
    let fs = with_files(1);
    for _ in 0..3 {
        let _ = list_all(&fs, "d");
    }
    assert_eq!(fs.calls("d", FakeOp::List), 3);
}

#[test]
fn counter_starts_at_zero() {
    assert_eq!(FakeFs::new().calls("d", FakeOp::List), 0);
}

#[test]
fn counter_counts_failed_calls() {
    let fs = FakeFs::new();
    let _ = list_all(&fs, "missing");
    assert_eq!(fs.calls("missing", FakeOp::List), 1);
}

#[test]
fn counter_ignores_a_leading_slash() {
    let fs = with_files(1);
    let _ = list_all(&fs, "/d");
    assert_eq!(fs.calls("d", FakeOp::List), 1);
}

fn fail_after_five() -> FakeFs {
    let fs = with_files(8).with_batch_size(2);
    let fault = Fault::FailAfterEntries {
        kind: FsErrorKind::NetworkLost,
        entries: 5,
    };
    fs.fault("d", FakeOp::List, 1, fault);
    fs
}

#[test]
fn fail_after_entries_streams_that_many() {
    assert_eq!(names(&list_all(&fail_after_five(), "d").1), file_names(5));
}

#[test]
fn fail_after_entries_keeps_batching_without_last() {
    assert_eq!(
        shape(&list_all(&fail_after_five(), "d").1),
        [(2, false), (2, false), (1, false)]
    );
}

#[test]
fn fail_after_entries_then_fails() {
    assert_eq!(
        kind_of(list_all(&fail_after_five(), "d").0),
        Some(FsErrorKind::NetworkLost)
    );
}

#[test]
fn fault_fires_once() {
    let fs = with_files(1);
    fs.fault("d", FakeOp::List, 1, Fault::Fail(FsErrorKind::Timeout));
    let _ = list_all(&fs, "d");
    assert_eq!(list_all(&fs, "d").0, Ok(()));
}

#[test]
fn fault_is_per_path() {
    let fs = with_files(0);
    fs.add_folder("e");
    fs.fault("d", FakeOp::List, 1, Fault::Fail(FsErrorKind::Timeout));
    assert_eq!(list_all(&fs, "e").0, Ok(()));
}

#[test]
fn fault_is_per_operation() {
    let fs = with_files(0);
    fs.fault("d", FakeOp::List, 1, Fault::Fail(FsErrorKind::Timeout));
    assert_eq!(metadata_kind(&fs, "d"), None);
}

#[test]
fn other_operation_does_not_advance_the_counter() {
    let fs = with_files(0);
    let _ = fs.metadata(&fs.location("d"));
    assert_eq!(fs.calls("d", FakeOp::List), 0);
}
