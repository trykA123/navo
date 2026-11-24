use navo_core::{EntryKind, Location, ProviderPath, Step};

use super::*;
use crate::browser::{PasteRefusal, ResultRow, ResultsRequest, RowStamp};

fn hits() -> Arc<FakeFs> {
    let fs = fake();
    fs.add_file("w/src/can/a.c", 3);
    fs.add_file("w/lib/a.c", 4);
    fs.add_file("w/top.c", 5);
    fs.add_file("other/b.c", 6);
    fs
}

const GONE: RowStamp = RowStamp {
    kind: EntryKind::File,
    size: 0,
    modified_ms: None,
    identity: None,
    created_ms: None,
};

fn row(fs: &FakeFs, path: &str) -> ResultRow {
    let location = fs.location(path);
    let stamp = RowStamp::read(fs, &location).unwrap_or(GONE);
    ResultRow { location, stamp }
}

fn request(fs: &FakeFs, kind: OpKind, sources: &[&str], target: &str) -> ResultsRequest {
    ResultsRequest {
        kind,
        rows: sources.iter().map(|path| row(fs, path)).collect(),
        base: fs.location("w"),
        target: fs.location(target),
    }
}

/// The message of the failed planning job `id`; nothing is pending.
fn failed(browser: &Browser, id: u64) -> String {
    assert!(browser.pending().is_none(), "a refused plan asks nothing");
    let job = browser.jobs().iter().find(|job| job.id == id);
    match job.map(|job| &job.status) {
        Some(JobStatus::Failed(message)) => message.clone(),
        other => panic!("expected a failed job, got {other:?}"),
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

#[test]
fn an_overwrite_never_replaces_another_selected_source() {
    let fs = hits();
    fs.add_file("w/d/a.c", 7);
    fs.add_file("out/d/a.c", 8);
    let request = request(&fs, OpKind::Move, &["w/d/a.c", "out/d/a.c"], "out");
    let (browser, id) = prepared(&fs, request);
    let message = failed(&browser, id);
    assert!(
        message.contains("d/a.c") && message.contains("selected"),
        "{message}"
    );
    assert!(exists(&fs, "w/d/a.c") && exists(&fs, "out/d/a.c"));
}

#[test]
fn a_row_changed_since_the_search_is_refused_by_name() {
    let grown = |fs: &FakeFs| fs.add_file("w/lib/a.c", 9);
    let now_a_folder = |fs: &FakeFs| {
        fs.remove("w/lib/a.c");
        fs.add_folder("w/lib/a.c");
    };
    for change in [&grown as &dyn Fn(&FakeFs), &now_a_folder] {
        let fs = hits();
        let request = request(&fs, OpKind::Move, &["w/top.c", "w/lib/a.c"], "dest");
        change(&fs);
        let (browser, id) = prepared(&fs, request);
        let message = failed(&browser, id);
        assert!(message.contains("lib/a.c"), "{message}");
        assert!(message.contains("changed since the search"), "{message}");
        assert!(exists(&fs, "w/top.c") && !exists(&fs, "dest/top.c"));
    }
}

#[test]
fn a_target_linked_into_a_source_is_refused() {
    let fs = hits();
    fs.make_link(&fs.location("w/src/can"), &fs.location("dest/can"))
        .expect("a link");
    let request = request(&fs, OpKind::Copy, &["w/src", "w/top.c"], "dest/can");
    let (browser, id) = prepared(&fs, request);
    let message = failed(&browser, id);
    assert!(message.contains("inside the selected folder"), "{message}");
    assert!(!exists(&fs, "w/src/can/top.c"));
}

#[test]
fn a_row_reached_through_a_link_is_planned_once() {
    let fs = hits();
    fs.make_link(&fs.location("w/lib"), &fs.location("w/alias"))
        .expect("a link");
    let mut request = request(&fs, OpKind::Copy, &["w/lib/a.c"], "dest");
    let stamp = request.rows[0].stamp;
    let location = fs.location("w/alias/a.c");
    request.rows.push(ResultRow { location, stamp });
    let (browser, _) = prepared(&fs, request);
    let pending = browser.pending().expect("asks before writing");
    assert_eq!(pending.sources, [fs.location("w/lib/a.c")]);
    assert_eq!(copies(&pending.plan).len(), 1);
}

#[test]
fn a_drive_relative_row_is_refused() {
    let fs = hits();
    let mut request = request(&fs, OpKind::Copy, &["w/top.c"], "dest");
    let location = Location::local(ProviderPath::from_text("C:foo")).expect("a location");
    request.rows.push(ResultRow {
        location,
        stamp: GONE,
    });
    let (browser, id) = prepared(&fs, request);
    let message = failed(&browser, id);
    assert!(
        message.contains("C:foo") && message.contains("not a full path"),
        "{message}"
    );
    assert!(!exists(&fs, "dest/top.c"));
}

#[test]
fn a_move_renames_only_when_every_row_shares_the_target_volume() {
    for sources in [["w/top.c", "other/b.c"], ["other/b.c", "w/top.c"]] {
        let fs = hits();
        fs.set_volume_serial("other", Some(2));
        let (mut browser, _) = prepared(&fs, request(&fs, OpKind::Move, &sources, "dest"));
        let plan = &browser.pending().expect("asks before writing").plan;
        let renamed = plan.steps.iter().any(|step| {
            matches!(step, Step::Rename { source, .. } if *source == fs.location("other/b.c"))
        });
        assert!(
            !renamed,
            "another volume is copied, then removed: {sources:?}"
        );
        assert!(browser.confirm());
        settle(&mut browser);
        assert!(exists(&fs, "dest/b.c") && exists(&fs, "dest/top.c"));
        assert!(!exists(&fs, "other/b.c") && !exists(&fs, "w/top.c"));
    }
}

#[test]
fn a_parent_and_child_move_moves_the_folder_once() {
    for sources in [["w/src", "w/src/can/a.c"], ["w/src/can/a.c", "w/src"]] {
        let fs = hits();
        let (mut browser, id) = prepared(&fs, request(&fs, OpKind::Move, &sources, "dest"));
        assert_eq!(
            browser.pending().map(|p| p.sources.clone()),
            Some(vec![fs.location("w/src")])
        );
        assert!(browser.confirm());
        settle(&mut browser);
        let job = browser.jobs().iter().find(|job| job.id == id);
        assert_eq!(job.map(|job| &job.status), Some(&JobStatus::Done));
        assert!(exists(&fs, "dest/src/can/a.c") && !exists(&fs, "w/src"));
    }
}

#[test]
fn a_link_navo_cannot_follow_refuses_the_plan() {
    let fs = hits();
    fs.add_folder("dest");
    fs.add_entry(
        "dest",
        navo_fs::FsEntry {
            name: ProviderPath::from_text("j"),
            kind: EntryKind::Link,
            size: 0,
            modified_ms: None,
            attributes: navo_fs::Attributes::default(),
        },
    );
    let (browser, id) = prepared(&fs, request(&fs, OpKind::Copy, &["w/top.c"], "dest/j"));
    let message = failed(&browser, id);
    assert!(
        message.contains("dest/j") && message.contains("could not follow"),
        "{message}"
    );
    assert!(exists(&fs, "w/top.c"));
}

#[test]
fn a_partial_target_is_refused() {
    let fs = hits();
    let mut request = request(&fs, OpKind::Copy, &["w/top.c"], "dest");
    request.target = Location::local(ProviderPath::from_text(r"\dest")).expect("a location");
    let (browser, id) = prepared(&fs, request);
    let message = failed(&browser, id);
    assert!(message.contains("not a full path"), "{message}");
    assert!(!exists(&fs, "dest/top.c"));
}

#[test]
fn a_same_size_replacement_or_another_folder_is_refused_as_changed() {
    let new_file = |fs: &FakeFs| {
        fs.remove("w/lib/a.c");
        fs.add_file("w/lib/a.c", 4);
    };
    let new_folder = |fs: &FakeFs| {
        fs.remove("w/src");
        fs.add_file("w/src/can/a.c", 3);
    };
    for (change, path) in [
        (&new_file as &dyn Fn(&FakeFs), "w/lib/a.c"),
        (&new_folder, "w/src"),
    ] {
        let fs = hits();
        let request = request(&fs, OpKind::Move, &["w/top.c", path], "dest");
        change(&fs);
        let (browser, id) = prepared(&fs, request);
        let message = failed(&browser, id);
        assert!(
            message.contains(path) && message.contains("changed since the search"),
            "{message}"
        );
        assert!(exists(&fs, "w/top.c") && !exists(&fs, "dest/top.c"));
    }
}

/// The message of the job `id` that ran and failed.
fn run_failed(browser: &Browser, id: u64) -> String {
    let job = browser.jobs().iter().find(|job| job.id == id);
    match job.map(|job| &job.status) {
        Some(JobStatus::Failed(message)) => message.clone(),
        other => panic!("expected a failed job, got {other:?}"),
    }
}

#[test]
fn a_source_replaced_after_planning_is_refused_on_confirm() {
    let fs = hits();
    let sources = ["w/top.c", "w/lib/a.c"];
    let (mut browser, id) = prepared(&fs, request(&fs, OpKind::Move, &sources, "dest"));
    fs.remove("w/lib/a.c");
    fs.add_file("w/lib/a.c", 4);
    assert!(browser.confirm());
    settle(&mut browser);
    let message = run_failed(&browser, id);
    assert!(
        message.contains("lib/a.c") && message.contains("changed since the search"),
        "{message}"
    );
    assert!(exists(&fs, "w/top.c") && exists(&fs, "w/lib/a.c"));
    assert!(!exists(&fs, "dest/top.c") && !exists(&fs, "dest/lib/a.c"));
}

#[test]
fn a_target_relinked_into_a_source_after_planning_is_refused_on_confirm() {
    let fs = hits();
    fs.add_folder("dest");
    let sources = ["w/src", "w/top.c"];
    let (mut browser, id) = prepared(&fs, request(&fs, OpKind::Copy, &sources, "dest"));
    fs.remove("dest");
    fs.make_link(&fs.location("w/src/can"), &fs.location("dest"))
        .expect("a link");
    assert!(browser.confirm());
    settle(&mut browser);
    let message = run_failed(&browser, id);
    assert!(message.contains("inside the selected folder"), "{message}");
    assert!(!exists(&fs, "w/src/can/top.c"));
}

/// True when the pending plan renames anything instead of copying, then removing.
fn renames(browser: &Browser) -> bool {
    let plan = &browser.pending().expect("asks before writing").plan;
    plan.steps
        .iter()
        .any(|step| matches!(step, Step::Rename { .. }))
}

#[test]
fn a_move_copies_when_a_kept_folder_sits_on_another_volume() {
    let fs = hits();
    fs.add_folder("dest/src");
    fs.set_volume_serial("dest/src", Some(2));
    let sources = ["w/src/can/a.c", "w/top.c"];
    let (mut browser, _) = prepared(&fs, request(&fs, OpKind::Move, &sources, "dest"));
    assert!(!renames(&browser));
    assert!(browser.confirm());
    settle(&mut browser);
    assert!(exists(&fs, "dest/src/can/a.c") && !exists(&fs, "w/src/can/a.c"));
}

#[test]
fn a_move_copies_when_a_volume_serial_is_unknown() {
    let fs = hits();
    fs.set_volume_serial("other", None);
    let sources = ["other/b.c", "w/top.c"];
    let (browser, _) = prepared(&fs, request(&fs, OpKind::Move, &sources, "dest"));
    assert!(!renames(&browser));
    let fs = hits();
    let (browser, _) = prepared(&fs, request(&fs, OpKind::Move, &sources, "dest"));
    assert!(renames(&browser), "one known volume still renames");
}

#[test]
fn an_overwrite_onto_a_second_name_of_a_source_is_refused() {
    let fs = hits();
    fs.add_file("z/lib/a.c", 4);
    fs.share_identity("z/lib/a.c", "w/lib/a.c");
    let request = request(&fs, OpKind::Move, &["w/lib/a.c", "w/top.c"], "z");
    let (browser, id) = prepared(&fs, request);
    let message = failed(&browser, id);
    assert!(
        message.contains("z/lib/a.c") && message.contains("selected source"),
        "{message}"
    );
    assert!(exists(&fs, "w/lib/a.c") && exists(&fs, "w/top.c"));
}

#[test]
fn a_target_inside_a_second_name_of_a_selected_folder_is_refused() {
    let fs = hits();
    fs.add_folder("z/can");
    fs.share_identity("z", "w/src");
    let request = request(&fs, OpKind::Copy, &["w/src", "w/top.c"], "z/can");
    let (browser, id) = prepared(&fs, request);
    let message = failed(&browser, id);
    assert!(message.contains("inside the selected folder"), "{message}");
    assert!(!exists(&fs, "z/can/top.c"));
}

#[test]
fn an_existing_target_navo_cannot_identify_is_refused() {
    let fs = hits();
    fs.add_file("dest/top.c", 9);
    for call in 1..=8 {
        let denied = Fault::Fail(FsErrorKind::AccessDenied);
        fs.fault("dest/top.c", FakeOp::Identity, call, denied);
    }
    let (browser, id) = prepared(&fs, request(&fs, OpKind::Copy, &["w/top.c"], "dest"));
    let message = failed(&browser, id);
    assert!(
        message.contains("dest/top.c") && message.contains("could not tell"),
        "{message}"
    );
    assert!(exists(&fs, "w/top.c"));
}
