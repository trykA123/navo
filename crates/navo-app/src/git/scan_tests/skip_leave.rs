use std::sync::Arc;

use navo_fs::DriveClass;

use super::skip_probe::{SkipProbe, fresh, harness, prints, repo, scanned};
use crate::git::scan::tests::{clean, location};

#[test]
fn leaving_a_folder_releases_its_watch_and_a_reopen_calls() {
    let (print, _) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    scanned(&h, "fixture", "repo");
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");

    h.scan.forget(&location("fixture"));
    assert!(
        probe.wait_dropped(1),
        "R3: a watch must not outlive the listing it was armed for"
    );
    h.observe("fixture", 2, &["repo"]);
    assert_eq!(h.next().0, repo(), "a new watch has no proof yet");
    h.reply(Ok(clean()));
    assert!(h.scan.stop());
}

#[test]
fn a_recheck_survives_a_newer_listing_before_its_status_finishes() {
    let (print, _) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    scanned(&h, "fixture", "repo");
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");

    h.scan.rescan(&location("fixture"));
    assert_eq!(h.next().0, repo(), "Recheck always calls");
    h.observe("fixture", 2, &["repo"]);
    assert_eq!(
        h.next().0,
        repo(),
        "the newer listing still owes the Recheck its status"
    );
    let before = h.revision("fixture");
    h.reply(Ok(clean()));
    h.wait("fixture", |badges| {
        badges.revision > before && !badges.by_name["repo"].stale
    });

    h.observe("fixture", 3, &["repo"]);
    h.wait("fixture", |badges| badges.by_name["repo"].stale);
    fresh(&h, "fixture", "repo");
    assert!(
        h.calls.try_recv().is_err(),
        "a finished Recheck forces nothing more"
    );
    assert!(h.scan.stop());
}
