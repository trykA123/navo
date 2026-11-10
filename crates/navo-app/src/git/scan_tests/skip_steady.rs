//! An unchanged recheck keeps its badge fresh and publishes nothing, so the badge never pulses.

use super::*;
use crate::git::scan::tests::Harness;

/// Opens a folder with one fresh, proved repo and returns the harness, probe and revision.
fn settled() -> (Harness, Arc<SkipProbe>, u64) {
    let (print, _) = prints();
    let probe = Arc::new(SkipProbe::new(DriveClass::Ssd, &["fixture/repo"]));
    probe.set_print(Some(print));
    let h = harness(probe.clone(), false);
    scanned(&h, "fixture", "repo");
    h.reply(Ok(clean()));
    fresh(&h, "fixture", "repo");
    let revision = h.revision("fixture");
    (h, probe, revision)
}

/// Gives the scanner time to act on a request, then reads the folder's badges.
fn after_recheck(h: &Harness) -> Arc<navo_ui::git::FolderBadges> {
    h.scan.rescan_changed(&location("fixture"));
    std::thread::sleep(Duration::from_millis(300));
    h.scan.badges(&location("fixture")).unwrap()
}

/// A recheck of an unchanged repository must never dim its badge: dim then restore is a pulse.
#[test]
fn an_unchanged_recheck_never_dims_the_badge() {
    let (h, _probe, _) = settled();
    let next = after_recheck(&h);
    assert!(
        !next.by_name["repo"].stale,
        "the first publish after an unchanged recheck dimmed the badge"
    );
    assert!(h.scan.stop());
}

/// An unchanged recheck publishes nothing, before or after the confirm wait, and calls no status.
#[test]
fn an_unchanged_recheck_publishes_nothing() {
    let (h, probe, revision) = settled();
    after_recheck(&h);
    assert_eq!(h.revision("fixture"), revision, "the recheck published");
    probe.advance(Duration::from_secs(2));
    {
        let _shared = h.scan.shared.0.lock().unwrap();
        h.scan.shared.1.notify_one();
    }
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(h.revision("fixture"), revision, "the confirm published");
    assert!(h.calls.try_recv().is_err(), "an unchanged recheck called");
    assert!(h.scan.stop());
}
