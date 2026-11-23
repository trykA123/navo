//! Speed cap on a stream from another provider (NV6b): the cap slows the streamed write too.
//! The local target runs on the real clock, so the test checks only a lower bound.

use std::num::NonZeroU64;
use std::time::{Duration, Instant};

use super::*;

#[test]
fn a_capped_stream_from_another_provider_waits_for_the_cap() {
    let scene = Scene::new(Feed::Whole, None);
    let mut harness = Harness {
        cancel: scene.cancel.clone(),
        ..Harness::default()
    };
    let started = Instant::now();
    let outcome = harness.with(&LocalFs::new(), |ctx| {
        let options = JobOptions {
            workers: 0,
            sources: Some(&scene.sources),
            verify: None,
            speed_cap: NonZeroU64::new(1024 * 1024),
        };
        run_job_with(&scene.plan, &[], ctx, options)
    });
    assert_eq!(outcome, JobOutcome::Done);
    assert_eq!(scene.target(), pattern());
    // 300 KiB at 1 MiB/s is 293 ms; the full bucket lets the first 100 ms through.
    let took = started.elapsed();
    assert!(took >= Duration::from_millis(150), "took {took:?}");
}
