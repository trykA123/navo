//! Tests for `run_job_with`: the options form must run a plan exactly as the older entry points do.

use super::*;

#[test]
fn run_job_with_four_workers_ends_like_run_job_parallel() {
    let ending = |with_options: bool| {
        let fs = fixture(&[
            ("src/a.txt", 2_500),
            ("src/b/c.txt", BIG),
            ("src/b/d.txt", 0),
        ]);
        let plan = copy_plan(&fs, &["src/a.txt", "src/b"], "dst");
        let mut harness = Harness::default();
        let outcome = harness.with(&fs, |ctx| match with_options {
            true => {
                let options = JobOptions {
                    workers: 4,
                    sources: None,
                    verify: None,
                };
                run_job_with(&plan, &[], ctx, options)
            }
            false => run_job_parallel(&plan, &[], ctx, 4),
        });
        let mut logged = harness.logged();
        logged.sort_by_key(|(seq, _)| *seq);
        (outcome, tree_of(&fs, ""), logged)
    };
    let expected = ending(false);
    assert_eq!(expected.0, JobOutcome::Done);
    assert_eq!(ending(true), expected);
}
