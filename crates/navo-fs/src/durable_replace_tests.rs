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
