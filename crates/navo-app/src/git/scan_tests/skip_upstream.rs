use std::sync::Arc;

use navo_fs::DriveClass;

use super::skip_probe::{SkipProbe, fresh, harness, prints, repo, upstream_status};

#[test]
fn a_first_new_upstream_leaves_no_proof_so_the_next_open_calls_and_the_one_after_reuses() {
    let (print, _) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    h.observe("fixture", 1, &["repo"]);
    h.next();
    h.reply(Ok(upstream_status("origin/main")));
    let first = fresh(&h, "fixture", "repo");
    assert_eq!(
        first.by_name["repo"].upstream.as_deref(),
        Some("origin/main")
    );

    let before = h.revision("fixture");
    h.observe("fixture", 2, &["repo"]);
    assert_eq!(
        h.next().0,
        repo(),
        "the upstream the first scan found earns no proof"
    );
    h.reply(Ok(upstream_status("origin/main")));
    h.wait("fixture", |badges| {
        badges.revision > before && !badges.by_name["repo"].stale
    });

    h.observe("fixture", 3, &["repo"]);
    h.wait("fixture", |badges| badges.by_name["repo"].stale);
    fresh(&h, "fixture", "repo");
    assert!(
        h.calls.try_recv().is_err(),
        "the third open reuses the proof from the second"
    );
    assert!(h.scan.stop());
}

#[test]
fn an_upstream_ref_change_between_opens_calls() {
    let (print, changed) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    h.observe("fixture", 1, &["repo"]);
    h.next();
    h.reply(Ok(upstream_status("origin/main")));
    fresh(&h, "fixture", "repo");
    let before = h.revision("fixture");
    h.observe("fixture", 2, &["repo"]);
    h.next();
    h.reply(Ok(upstream_status("origin/main")));
    h.wait("fixture", |badges| {
        badges.revision > before && !badges.by_name["repo"].stale
    });

    probe.set_print(Some(changed));
    h.observe("fixture", 3, &["repo"]);
    h.wait("fixture", |badges| badges.by_name["repo"].stale);
    assert_eq!(h.next().0, repo(), "the ref moved under the proof");
    h.reply(Ok(upstream_status("origin/main")));
    assert!(h.scan.stop());
}
