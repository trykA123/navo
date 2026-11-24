use navo_core::{Location, Step};

use super::*;
use crate::browser::{PasteRefusal, ResultsRequest};

fn hits() -> Arc<FakeFs> {
    let fs = fake();
    fs.add_file("w/src/can/a.c", 3);
    fs.add_file("w/lib/a.c", 4);
    fs.add_file("w/top.c", 5);
    fs.add_file("other/b.c", 6);
    fs
}

fn request(fs: &FakeFs, kind: OpKind, sources: &[&str], target: &str) -> ResultsRequest {
    ResultsRequest {
        kind,
        sources: sources.iter().map(|path| fs.location(path)).collect(),
        base: fs.location("w"),
        target: fs.location(target),
    }
}

fn prepared(fs: &Arc<FakeFs>, request: ResultsRequest) -> (Browser, u64) {
    let mut browser = browser(fs, AppState::new(fs.location("home")));
    let id = browser.prepare_from_results(request).expect("planned");
    settle(&mut browser);
    (browser, id)
}

fn copies(plan: &OperationPlan) -> Vec<(&Location, &Location)> {
    plan.steps
        .iter()
        .filter_map(|step| match step {
            Step::CopyFile { source, target, .. } | Step::Rename { source, target } => {
                Some((source, target))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn sources_from_two_folders_keep_the_structure_and_ask() {
    let fs = hits();
    let sources = ["w/src/can/a.c", "w/lib/a.c"];
    let (mut browser, id) = prepared(&fs, request(&fs, OpKind::Copy, &sources, "dest"));
    let pending = browser.pending().expect("asks before writing");
    assert_eq!(pending.id, id);
    let structure = pending.structure.as_ref().expect("a structure choice");
    assert_eq!(structure.base, fs.location("w"));
    assert!(structure.keep);
    assert!(!exists(&fs, "dest/lib/a.c"));
    assert!(browser.confirm());
    settle(&mut browser);
    assert!(exists(&fs, "dest/src/can/a.c"));
    assert!(exists(&fs, "dest/lib/a.c"));
}

#[test]
fn sources_all_in_the_base_offer_no_structure() {
    let fs = hits();
    let (browser, _) = prepared(&fs, request(&fs, OpKind::Copy, &["w/top.c"], "dest"));
    let pending = browser.pending().expect("asks before writing");
    assert!(pending.structure.is_none());
}

#[test]
fn a_target_inside_a_source_is_refused() {
    let fs = hits();
    let mut browser = browser(&fs, AppState::new(fs.location("home")));
    let request = request(&fs, OpKind::Copy, &["w/src", "w/top.c"], "w/src/can");
    assert_eq!(
        browser.prepare_from_results(request),
        Err(PasteRefusal::IntoItself)
    );
    assert!(browser.pending().is_none());
}

#[test]
fn only_copy_and_move_are_planned() {
    let fs = hits();
    let mut browser = browser(&fs, AppState::new(fs.location("home")));
    for kind in [OpKind::Link, OpKind::Delete, OpKind::Recycle] {
        let request = request(&fs, kind, &["w/top.c"], "dest");
        assert_eq!(
            browser.prepare_from_results(request),
            Err(PasteRefusal::Nothing)
        );
    }
    assert!(
        browser
            .prepare_from_results(request(&fs, OpKind::Copy, &[], "dest"))
            .is_err()
    );
}

#[test]
fn duplicate_rows_plan_each_file_once() {
    let fs = hits();
    let sources = ["w/lib/a.c", "w/top.c", "w/lib/a.c"];
    let (browser, _) = prepared(&fs, request(&fs, OpKind::Copy, &sources, "dest"));
    let pending = browser.pending().expect("asks before writing");
    assert_eq!(
        pending.sources,
        [fs.location("w/lib/a.c"), fs.location("w/top.c")]
    );
    assert!(pending.plan.conflicts.is_empty());
    assert_eq!(copies(&pending.plan).len(), 2);
}

#[test]
fn a_row_inside_another_rows_folder_is_planned_once() {
    let fs = hits();
    for sources in [["w/src", "w/src/can/a.c"], ["w/src/can/a.c", "w/src"]] {
        let (browser, _) = prepared(&fs, request(&fs, OpKind::Copy, &sources, "dest"));
        let pending = browser.pending().expect("asks before writing");
        assert_eq!(pending.sources, [fs.location("w/src")]);
        assert!(pending.plan.conflicts.is_empty());
        assert_eq!(copies(&pending.plan).len(), 1);
    }
}

#[test]
fn a_vanished_row_fails_the_plan_visibly_and_writes_nothing() {
    let fs = hits();
    let sources = ["w/top.c", "w/gone.c"];
    let (browser, id) = prepared(&fs, request(&fs, OpKind::Copy, &sources, "dest"));
    assert!(browser.pending().is_none());
    let job = browser
        .jobs()
        .iter()
        .find(|job| job.id == id)
        .expect("a job");
    assert!(matches!(job.status, JobStatus::Failed(_)));
    assert!(!exists(&fs, "dest/top.c"));
}

#[test]
fn mixed_sources_from_two_roots_stay_inside_the_target() {
    let fs = hits();
    let sources = ["w/src/can/a.c", "other/b.c", "w/top.c"];
    let (mut browser, _) = prepared(&fs, request(&fs, OpKind::Move, &sources, "dest"));
    let pending = browser.pending().expect("asks before writing");
    assert_eq!(pending.plan.kind, OpKind::Move);
    let dest = fs.location("dest");
    for (_, target) in copies(&pending.plan) {
        let inside = std::iter::successors(Some(target.clone()), Location::parent)
            .any(|place| place.same_place(&dest));
        assert!(inside, "{target:?}");
    }
    assert!(browser.confirm());
    settle(&mut browser);
    assert!(exists(&fs, "dest/src/can/a.c"));
    assert!(exists(&fs, "dest/b.c"));
    assert!(exists(&fs, "dest/top.c"));
    assert!(!exists(&fs, "other/b.c"));
}

#[test]
fn a_target_at_the_base_never_writes_over_a_source() {
    let fs = hits();
    let (browser, _) = prepared(&fs, request(&fs, OpKind::Copy, &["w/src/can/a.c"], "w"));
    let pending = browser.pending().expect("asks before writing");
    for (source, target) in copies(&pending.plan) {
        assert!(!source.same_place(target), "{target:?}");
    }
}
