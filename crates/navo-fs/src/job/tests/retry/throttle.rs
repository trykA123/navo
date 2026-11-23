//! Speed cap (NV6b): a job capped at 1 MiB/s copies 4 MiB in about 4 s on the fake clock, a job
//! with no cap never waits, and a cancel during a cap wait stops the job at once.

use std::num::NonZeroU64;
use std::time::Duration;

use super::*;

const MIB: u64 = 1024 * 1024;
const BIG_FILE: &str = "src/big.bin";

/// A fake provider holding one 4 MiB source file.
fn big_file() -> FakeFs {
    fixture(&[(BIG_FILE, 4 * MIB)])
}

/// Copies the 4 MiB file on `fs` with `workers` and the speed cap `cap` in bytes per second.
fn copy_capped(
    fs: &FakeFs,
    (workers, cap): (usize, Option<u64>),
    harness: &mut Harness,
) -> JobOutcome {
    let plan = copy_plan(fs, &[BIG_FILE], "dst");
    harness.with(fs, |ctx| {
        let options = JobOptions {
            workers,
            sources: None,
            verify: None,
            speed_cap: cap.and_then(NonZeroU64::new),
        };
        run_job_with(&plan, &[], ctx, options)
    })
}

#[test]
fn a_cap_of_one_mib_per_second_copies_four_mib_in_about_four_seconds() {
    for workers in MODES {
        let fs = big_file();
        let outcome = copy_capped(&fs, (workers, Some(MIB)), &mut Harness::default());
        assert_eq!(outcome, JobOutcome::Done, "{workers} workers");
        assert_eq!(size(&fs, "dst/big.bin"), Some(4 * MIB), "{workers} workers");
        let waited: Duration = fs.retry_waits().iter().sum();
        let about_four = Duration::from_millis(3_800)..=Duration::from_secs(4);
        assert!(
            about_four.contains(&waited),
            "{workers} workers waited {waited:?}"
        );
    }
}

#[test]
fn a_job_with_no_cap_never_waits() {
    for workers in MODES {
        let fs = big_file();
        let outcome = copy_capped(&fs, (workers, None), &mut Harness::default());
        assert_eq!(outcome, JobOutcome::Done, "{workers} workers");
        assert_eq!(size(&fs, "dst/big.bin"), Some(4 * MIB), "{workers} workers");
        assert_eq!(fs.retry_waits(), [], "{workers} workers");
    }
}

#[test]
fn a_cancel_during_a_cap_wait_stops_the_job() {
    for workers in MODES {
        let (fs, mut harness) = (big_file(), Harness::default());
        fs.cancel_during_next_wait(harness.cancel.clone());
        let outcome = copy_capped(&fs, (workers, Some(MIB)), &mut harness);
        assert_eq!(outcome, JobOutcome::Cancelled, "{workers} workers");
        assert_eq!(fs.retry_waits().len(), 1, "{workers} workers");
        let sent = harness.progress.iter().map(|seen| seen.bytes_done).max();
        assert!(sent < Some(MIB), "{workers} workers sent {sent:?}");
        assert!(!exists(&fs, "dst/big.bin"), "{workers} workers");
        assert!(leftovers(&fs).is_empty(), "{workers} workers");
    }
}
