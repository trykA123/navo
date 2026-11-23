//! CS2: the copy dialog shows "N already there" and one conflict choice row; a pick by click or
//! by Left/Right rebuilds the plan, Enter starts the job, and overwrite choices hide when Undo
//! could not restore an overwritten file.

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use navo_core::ConflictPolicy;

use super::confirm_props;
use crate::browser::Operations;
use crate::window_view::NavoWindow;
use crate::window_view::tests::{fake, home_beside_dest, open_options, options_for, press, settle};

const ALL: [&str; 4] = [
    "conflict-skip",
    "conflict-overwrite-older",
    "conflict-keep-both",
    "conflict-overwrite-all",
];

/// Opens a window whose cursor folder `home/docs` has three files `dest/docs` already holds,
/// then presses F5; `undoes` says whether jobs can undo an overwrite.
fn copy_dialog(cx: &mut TestAppContext, undoes: bool) -> (AnyWindowHandle, Entity<NavoWindow>) {
    let fs = fake();
    for name in ["x.txt", "y.txt"] {
        fs.add_file(&format!("home/docs/{name}"), 3);
    }
    for name in ["report.txt", "x.txt", "y.txt"] {
        fs.add_file(&format!("dest/docs/{name}"), 1);
    }
    let state = home_beside_dest(&fs);
    let options = options_for(fs, state);
    let options = crate::window_view::StartOptions {
        operations: Operations {
            undoes_overwrites: undoes,
            ..Operations::unlogged()
        },
        ..options
    };
    let (window, view) = open_options(cx, options);
    press(cx, window, &view, "f5");
    (window, view)
}

/// The summary line the dialog shows for the pending plan.
fn summary(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> String {
    view.read_with(cx, |view, _| {
        let pending = view.browser.pending().expect("a pending plan");
        confirm_props(pending).summary.to_string()
    })
}

/// The summary's second line, which names the conflicts; the first line keeps only the sizes.
fn conflict_line(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> Option<String> {
    let summary = summary(cx, view);
    let (sizes, conflicts) = summary.split_once('\n')?;
    assert!(!sizes.contains("already there"), "{summary}");
    Some(conflicts.to_owned())
}

/// The picked conflict policy of the pending plan.
fn policy(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> Option<ConflictPolicy> {
    view.read_with(cx, |view, _| {
        view.browser.pending().map(|p| p.replan.policy.clone())
    })
}

/// Renders a frame and says which of `ids` the dialog draws, and whether each is picked.
fn drawn(cx: &mut TestAppContext, window: AnyWindowHandle) -> Vec<(&'static str, bool)> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        ALL.iter()
            .filter_map(|id| {
                let found = window.try_find(*id)?;
                Some((*id, found.selected() == Some(true)))
            })
            .collect()
    })
    .expect("window")
}

/// Clicks the element `id` and lets the rebuilt plan land.
fn click(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<NavoWindow>,
    id: &'static str,
) {
    cx.update_window(window, |_, window, cx| window.click(id, cx))
        .expect("window");
    settle(cx, view);
}

#[gpui_kit::test]
fn picking_keep_both_turns_already_there_into_new_names(cx: &mut TestAppContext) {
    let (window, view) = copy_dialog(cx, true);
    assert_eq!(
        conflict_line(cx, &view).as_deref(),
        Some("3 items already there will be skipped")
    );
    click(cx, window, &view, "conflict-keep-both");
    assert_eq!(policy(cx, &view), Some(ConflictPolicy::Rename));
    assert_eq!(
        conflict_line(cx, &view).as_deref(),
        Some("3 items already there: 3 get new names")
    );
    let picked = drawn(cx, window);
    assert!(picked.contains(&("conflict-keep-both", true)), "{picked:?}");
}

#[gpui_kit::test]
fn the_row_shows_four_choices_in_order_with_skip_picked(cx: &mut TestAppContext) {
    let (window, _) = copy_dialog(cx, true);
    let row = drawn(cx, window);
    let ids: Vec<_> = row.iter().map(|(id, _)| *id).collect();
    assert_eq!(ids, ALL);
    assert_eq!(row[0], ("conflict-skip", true));
}

#[gpui_kit::test]
fn overwrite_choices_hide_when_undo_cannot_restore_them(cx: &mut TestAppContext) {
    let (window, _) = copy_dialog(cx, false);
    let ids: Vec<_> = drawn(cx, window).iter().map(|(id, _)| *id).collect();
    assert_eq!(ids, ["conflict-skip", "conflict-keep-both"]);
}

#[gpui_kit::test]
fn tab_reaches_the_row_arrows_pick_and_enter_starts_the_job(cx: &mut TestAppContext) {
    let (window, view) = copy_dialog(cx, true);
    let focused = |cx: &mut TestAppContext| {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.find("options").focused() == Some(true)
        })
        .expect("window")
    };
    assert!(!focused(cx));
    press(cx, window, &view, "tab");
    press(cx, window, &view, "tab");
    assert!(focused(cx));
    press(cx, window, &view, "right");
    assert_eq!(policy(cx, &view), Some(ConflictPolicy::OverwriteOlder));
    press(cx, window, &view, "right");
    assert_eq!(policy(cx, &view), Some(ConflictPolicy::Rename));
    press(cx, window, &view, "left");
    assert_eq!(policy(cx, &view), Some(ConflictPolicy::OverwriteOlder));
    assert!(focused(cx));
    press(cx, window, &view, "enter");
    assert_eq!(policy(cx, &view), None);
    let jobs = view.read_with(cx, |view, _| view.job_infos().len());
    assert_eq!(jobs, 1);
}

#[gpui_kit::test]
fn a_plan_without_conflicts_shows_no_row(cx: &mut TestAppContext) {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let (window, view) = open_options(cx, options_for(fs, state));
    press(cx, window, &view, "f5");
    assert!(view.read_with(cx, |view, _| view.browser.pending().is_some()));
    assert!(drawn(cx, window).is_empty());
}

#[gpui_kit::test]
fn space_on_the_row_does_not_start_the_job_but_enter_does(cx: &mut TestAppContext) {
    let (window, view) = copy_dialog(cx, true);
    press(cx, window, &view, "tab");
    press(cx, window, &view, "tab");
    press(cx, window, &view, "space");
    assert_eq!(policy(cx, &view), Some(ConflictPolicy::Ask));
    assert_eq!(view.read_with(cx, |view, _| view.job_infos().len()), 0);
    press(cx, window, &view, "enter");
    assert_eq!(policy(cx, &view), None);
    assert_eq!(view.read_with(cx, |view, _| view.job_infos().len()), 1);
}

#[test]
fn an_asking_plan_highlights_what_it_really_does() {
    let at = |path: &str| navo_core::Location::new(path).expect("a location");
    let clash = |resolution| navo_core::Conflict {
        source: at("/home/x.txt"),
        target: at("/home/x.txt"),
        resolution,
    };
    let mut plan = navo_core::OperationPlan {
        kind: navo_core::OpKind::Copy,
        steps: Vec::new(),
        totals: navo_core::Totals {
            files: 1,
            folders: 0,
            bytes: 1,
        },
        conflicts: vec![clash(navo_core::Resolution::Rename(at("/home/x (2).txt")))],
        risks: Vec::new(),
    };
    let shown =
        |plan: &navo_core::OperationPlan, policy| super::conflicts::shown_policy(&policy, plan);
    assert_eq!(shown(&plan, ConflictPolicy::Ask), ConflictPolicy::Rename);
    assert_eq!(shown(&plan, ConflictPolicy::Skip), ConflictPolicy::Skip);
    plan.conflicts = vec![clash(navo_core::Resolution::Ask)];
    assert_eq!(shown(&plan, ConflictPolicy::Ask), ConflictPolicy::Skip);
    assert_eq!(
        shown(&plan, ConflictPolicy::OverwriteAll),
        ConflictPolicy::OverwriteAll
    );
}
