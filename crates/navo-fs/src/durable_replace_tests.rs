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
