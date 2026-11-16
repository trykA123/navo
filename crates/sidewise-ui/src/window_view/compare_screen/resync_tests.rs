//! The resync decision as a pure step (N4f1): skip an unchanged stamp, wait out the throttle
//! while the compare runs, rebuild a new generation or a finished compare at once.

use std::time::{Duration, Instant};

use super::resync::{ResyncStep, resync_step};

#[test]
fn resync_skips_an_unchanged_stamp() {
    let now = Instant::now();
    let step = resync_step(Some((1, 5)), Some(now), (1, 5), true, now);
    assert_eq!(step, ResyncStep::Skip);
}

#[test]
fn resync_rebuilds_a_new_generation_at_once() {
    let now = Instant::now();
    let step = resync_step(Some((1, 5)), Some(now), (2, 0), true, now);
    assert_eq!(step, ResyncStep::Rebuild { fresh: true });
}

#[test]
fn resync_waits_while_checking_inside_500_ms() {
    let now = Instant::now();
    let last = now - Duration::from_millis(200);
    let step = resync_step(Some((1, 5)), Some(last), (1, 9), true, now);
    assert_eq!(step, ResyncStep::Wait(Duration::from_millis(300)));
}

#[test]
fn resync_rebuilds_a_finished_compare_at_once() {
    let now = Instant::now();
    let step = resync_step(Some((1, 5)), Some(now), (1, 9), false, now);
    assert_eq!(step, ResyncStep::Rebuild { fresh: false });
}
