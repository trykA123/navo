use std::sync::Arc;

use rusty_core::find::{FileHits, FindQuery};
use rusty_core::{Location, ProviderPath, Step};
use rusty_fs::find::{FindProgress, FoundBatch, FoundFile};
use rusty_fs::{Attributes, EntryKind, FsEntry};

use super::*;

const TWINS: [&str; 2] = ["src/can/a.c", "lib/a.c"];

fn found(names: &'static [&'static str]) -> FoundBatch {
    FoundBatch {
        files: names
            .iter()
            .map(|name| FoundFile {
                relative: ProviderPath::from_text(name),
                entry: FsEntry {
                    name: ProviderPath::from_text("a.c"),
                    kind: EntryKind::File,
                    size: 1,
                    modified_ms: None,
                    attributes: Attributes::default(),
                },
                hits: FileHits::default(),
            })
            .collect(),
        progress: FindProgress::default(),
    }
}

fn results(names: &'static [&'static str]) -> (Arc<FakeFs>, Browser) {
    let fs = fake();
    fs.add_file("w/src/can/a.c", 3);
    fs.add_file("w/lib/a.c", 4);
    fs.add_file("w/top.c", 5);
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.find_walk = Arc::new(move |_, _, _, sink| {
        let _ = sink(found(names));
        Ok(FindProgress::default())
    });
    let pane = browser.state.focused_pane();
    browser.start_find(pane, FindQuery::parse("x"), fs.location("w"));
    settle(&mut browser);
    for _ in names {
        browser.command(&Command::SelectionToggleDown).unwrap();
    }
    (fs, browser)
}

fn planned(browser: &mut Browser, kind: OpKind) -> &super::super::PendingOp {
    assert!(browser.prepare(kind));
    settle(browser);
    browser.pending().expect("a plan")
}

fn places(step: &Step) -> Vec<&Location> {
    match step {
        Step::CopyFile { source, target, .. } | Step::Rename { source, target } => {
            vec![source, target]
        }
        Step::MakeFolder { target } => vec![target],
        Step::RemoveFile { location }
        | Step::RemoveFolder { location }
        | Step::RemoveLink { location }
        | Step::Recycle { location } => vec![location],
        Step::MakeLink { target_of, at } => vec![target_of, at],
    }
}

fn assert_no_find(pending: &super::super::PendingOp) {
    let plan = &pending.plan;
    let all = plan
        .steps
        .iter()
        .flat_map(places)
        .chain(plan.conflicts.iter().flat_map(|c| [&c.source, &c.target]))
        .chain(&pending.sources)
        .chain(&pending.target);
    for place in all {
        assert_ne!(place.provider().as_str(), "find", "{place:?}");
    }
}

#[test]
fn copy_from_results_keeps_the_folder_structure_by_default() {
    let (fs, mut browser) = results(&TWINS);
    let pending = planned(&mut browser, OpKind::Copy);
    assert_eq!(
        pending.sources,
        [fs.location("w/src/can/a.c"), fs.location("w/lib/a.c")]
    );
    assert!(pending.structure.as_ref().is_some_and(|s| s.keep));
    assert_no_find(pending);
    assert!(browser.confirm());
    settle(&mut browser);
    assert!(exists(&fs, "dest/src/can/a.c"));
    assert!(exists(&fs, "dest/lib/a.c"));
    assert!(!exists(&fs, "dest/a.c"));
}

#[test]
fn flat_copy_asks_about_the_twin_and_never_overwrites_it() {
    let (fs, mut browser) = results(&TWINS);
    planned(&mut browser, OpKind::Copy);
    assert!(browser.toggle_structure());
    settle(&mut browser);
    let pending = browser.pending().expect("a new plan");
    assert!(pending.structure.as_ref().is_some_and(|s| !s.keep));
    assert_eq!(pending.plan.conflicts.len(), 1);
    assert_eq!(pending.plan.conflicts[0].source, fs.location("w/lib/a.c"));
    assert_no_find(pending);
    assert!(browser.confirm());
    settle(&mut browser);
    let landed = fs.metadata(&fs.location("dest/a.c")).unwrap();
    assert_eq!(landed.size, 3);
    assert!(!exists(&fs, "dest/src"));
}

#[test]
fn a_move_from_results_plans_real_locations_only() {
    let (fs, mut browser) = results(&TWINS);
    let pending = planned(&mut browser, OpKind::Move);
    assert_no_find(pending);
    assert!(
        pending
            .plan
            .steps
            .iter()
            .any(|step| matches!(step, Step::Rename { .. }))
    );
    browser.confirm();
    settle(&mut browser);
    assert!(exists(&fs, "dest/src/can/a.c"));
    assert!(!exists(&fs, "w/src/can/a.c"));
    assert!(exists(&fs, "w/top.c"));
}

#[test]
fn confirm_waits_while_the_choice_replans_and_a_cancel_stays_closed() {
    let (fs, mut browser) = results(&TWINS);
    planned(&mut browser, OpKind::Copy);
    assert!(browser.toggle_structure());
    assert!(
        browser
            .pending()
            .is_some_and(super::super::PendingOp::is_planning)
    );
    assert!(!browser.confirm());
    assert!(browser.dismiss());
    settle(&mut browser);
    assert!(browser.pending().is_none());
    assert!(!exists(&fs, "dest/a.c"));
}

#[test]
fn the_choice_is_offered_only_for_results_with_relative_folders() {
    let (_, mut browser) = results(&["top.c"]);
    assert!(planned(&mut browser, OpKind::Copy).structure.is_none());
    let fs = fake();
    let mut plain = browser_at_home(&fs);
    assert!(planned(&mut plain, OpKind::Copy).structure.is_none());
}

fn browser_at_home(fs: &Arc<FakeFs>) -> Browser {
    browser(fs, home_beside_dest(fs))
}
