use std::time::{Duration, Instant};

use crate::model::{Budget, CancelToken};

use super::super::super::*;
use super::super::{covered, summary};
use super::{assert_within, request_budget, side, slow};

const MIB: usize = 1024 * 1024;

/// Two unequal unterminated ASCII lines of `bytes` each, diffed under `budget`.
fn unequal_lines(bytes: usize, budget: Budget) {
    let diff = LargeDiff::start_tuned(
        request_budget(
            side(&vec![b'a'; bytes]),
            side(&vec![b'b'; bytes]),
            0,
            budget,
        ),
        &CancelToken::new(),
        slow(),
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut events = Vec::new();
    loop {
        let event = diff
            .events()
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("the diff finishes instead of waiting for memory");
        let done = matches!(event, DiffEvent::Done(_));
        events.push(event);
        if done {
            break;
        }
    }
    summary(&events);
    assert!(!covered(&events).is_empty());
    assert_within(&diff, budget);
}

#[test]
fn unequal_3_mib_lines_on_a_17_mib_budget_finish_within_it() {
    unequal_lines(3 * MIB, Budget::from_mib(17));
}

#[test]
fn unequal_6_5_mib_lines_on_the_32_mib_budget_finish_within_it() {
    unequal_lines(6 * MIB + MIB / 2, Budget::from_mib(32));
}
