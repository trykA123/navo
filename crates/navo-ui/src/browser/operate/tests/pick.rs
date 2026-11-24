//! CS2 and CS3: a picked conflict policy rebuilds the pending plan through the planning job. A
//! rebuild for an older pick or a closed dialog is dropped, each plan says whether Undo can put an
//! overwritten file back (the overwrite choices stay open either way), and the plan carries both
//! sides of its first 200 conflicts.

use navo_core::{ConflictPolicy, Resolution};

use super::super::{OpRequest, PendingOp};
use super::*;

/// A browser whose cursor folder `home/docs` holds three files that `dest/docs` already has,
/// with the copy dialog open; `undoes` says whether its jobs can undo an overwrite.
fn clashing(undoes: bool) -> (Arc<FakeFs>, Browser) {
    let fs = fake();
    for name in ["x.txt", "y.txt"] {
        fs.add_file(&format!("home/docs/{name}"), 3);
    }
    for name in ["report.txt", "x.txt", "y.txt"] {
        fs.add_file(&format!("dest/docs/{name}"), 1);
    }
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.set_operations(Operations {
        retain_overwrites: Some(Arc::new(move || undoes)),
        ..Operations::unlogged()
    });
    assert!(browser.prepare(OpKind::Copy));
    settle(&mut browser);
    (fs, browser)
}

/// The resolution of each conflict in the pending plan.
fn resolutions(browser: &Browser) -> Vec<Resolution> {
    let pending = browser.pending().expect("a pending plan");
    pending
        .plan
        .conflicts
        .iter()
        .map(|c| c.resolution.clone())
        .collect()
}

/// A rebuild request for `pending` with `policy`, as `pick_policy` sends it.
fn request_for(pending: &PendingOp, policy: ConflictPolicy) -> OpRequest {
    OpRequest {
        id: pending.id,
        kind: pending.plan.kind,
        sources: pending.sources.clone(),
        target: pending.target.clone(),
        same_volume: false,
        protected: Vec::new(),
        source_tab: pending.source_tab,
        asks: true,
        structure: None,
        stream: None,
        policy,
    }
}

#[test]
fn keep_both_rebuilds_the_plan_and_confirm_waits_for_it() {
    let (_, mut browser) = clashing(false);
    assert_eq!(resolutions(&browser), vec![Resolution::Ask; 3]);
    assert!(browser.pick_policy(ConflictPolicy::Rename));
    assert!(browser.pending().is_some_and(PendingOp::is_planning));
    assert!(!browser.confirm());
    settle(&mut browser);
    let pending = browser.pending().expect("the rebuilt plan");
    assert!(!pending.is_planning());
    assert_eq!(pending.replan.policy, ConflictPolicy::Rename);
    let renamed = resolutions(&browser);
    assert!(
        renamed.iter().all(|r| matches!(r, Resolution::Rename(_))),
        "{renamed:?}"
    );
}

#[test]
fn a_rebuild_for_another_pick_or_a_closed_dialog_is_stale() {
    let (_, mut browser) = clashing(false);
    assert!(browser.pick_policy(ConflictPolicy::Rename));
    let pending = browser.pending().expect("a pending plan").clone();
    assert!(browser.is_stale_replan(&request_for(&pending, ConflictPolicy::Skip)));
    assert!(!browser.is_stale_replan(&request_for(&pending, ConflictPolicy::Rename)));
    assert!(browser.dismiss());
    assert!(browser.is_stale_replan(&request_for(&pending, ConflictPolicy::Rename)));
    settle(&mut browser);
    assert!(browser.pending().is_none());
}

#[test]
fn the_last_pick_wins_when_two_rebuilds_race() {
    let (_, mut browser) = clashing(false);
    assert!(browser.pick_policy(ConflictPolicy::Rename));
    assert!(browser.pick_policy(ConflictPolicy::Skip));
    settle(&mut browser);
    assert_eq!(
        browser.pending().map(|p| p.replan.policy.clone()),
        Some(ConflictPolicy::Skip)
    );
    assert_eq!(resolutions(&browser), vec![Resolution::Skip; 3]);
}

#[test]
fn overwrites_may_be_picked_when_undo_cannot_restore_them() {
    let (_, mut browser) = clashing(false);
    assert!(browser.pending().is_some_and(|p| !p.undoes_overwrites));
    assert!(browser.pick_policy(ConflictPolicy::OverwriteAll));
    settle(&mut browser);
    assert_eq!(resolutions(&browser), vec![Resolution::Overwrite; 3]);
    let (_, browser) = clashing(true);
    assert!(browser.pending().is_some_and(|p| p.undoes_overwrites));
}

#[test]
fn each_plan_reads_the_retention_setting_when_it_is_built() {
    let keeps = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let (_, mut browser) = clashing(true);
    let read = Arc::clone(&keeps);
    browser.set_operations(Operations {
        retain_overwrites: Some(Arc::new(move || {
            read.load(std::sync::atomic::Ordering::SeqCst)
        })),
        ..Operations::unlogged()
    });
    keeps.store(false, std::sync::atomic::Ordering::SeqCst);
    assert!(browser.pick_policy(ConflictPolicy::OverwriteAll));
    settle(&mut browser);
    assert!(browser.pending().is_some_and(|p| !p.undoes_overwrites));
}

#[test]
fn a_profile_without_the_check_never_claims_undo() {
    let fs = fake();
    fs.add_file("home/docs/x.txt", 3);
    fs.add_file("dest/docs/x.txt", 1);
    let mut browser = browser(&fs, home_beside_dest(&fs));
    assert!(browser.prepare(OpKind::Copy));
    settle(&mut browser);
    assert!(browser.pending().is_some_and(|p| !p.undoes_overwrites));
}

#[test]
fn the_plan_carries_both_sides_of_its_conflicts() {
    let (_, browser) = clashing(false);
    let pending = browser.pending().expect("a pending plan");
    assert_eq!(pending.clashes.len(), pending.plan.conflicts.len());
    let x = pending.clashes.iter().find(|c| c.name == "x.txt");
    let x = x.expect("x.txt is listed");
    assert_eq!((x.source.size, x.target.size), (Some(3), Some(1)));
    let read = |f: &crate::browser::Facts| f.is_folder || f.size.is_some();
    let all_read = pending
        .clashes
        .iter()
        .all(|c| read(&c.source) && read(&c.target));
    assert!(all_read, "{:?}", pending.clashes);
}

#[test]
fn the_list_holds_at_most_the_first_200_conflicts() {
    let (fs, browser) = clashing(false);
    let mut plan = browser.pending().expect("a pending plan").plan.clone();
    let first = plan.conflicts[0].clone();
    plan.conflicts = vec![first; super::super::CLASHES_SHOWN + 1];
    let listed = super::super::clashes::clashes(fs.as_ref(), &plan);
    assert_eq!(listed.len(), 200);
}
