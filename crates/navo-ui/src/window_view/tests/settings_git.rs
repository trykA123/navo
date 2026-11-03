use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, Bounds, ElementId, Pixels, TestAppContext, px};

use super::app_menu::{Rig, rig_git, rig_git_sized};
use super::git_menu::{Fake, menu_items, modified};
use crate::settings::{Change, FIELDS, Field, Values};

const AUTO: usize = 0;
const FAKE: usize = 1;
const OTHER: usize = 3;

fn open(cx: &mut TestAppContext, values: Values, git: &Fake) -> Rig {
    let rig = rig_git(cx, values, Arc::new(git.clone()));
    let view = rig.view.clone();
    cx.update_window(rig.window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.open_settings_at(Field::GitTool, window, cx)
        });
    })
    .expect("window");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    rig
}

fn fake() -> Fake {
    Fake::new(menu_items(false), modified())
}

fn label(cx: &mut TestAppContext, rig: &Rig, id: impl Into<ElementId>) -> Option<String> {
    let id = id.into();
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .and_then(|found| found.label().map(str::to_owned))
    })
    .expect("window")
}

fn command_row() -> (&'static str, usize) {
    let at = FIELDS
        .iter()
        .position(|spec| spec.field == Field::GitToolCommand);
    ("settings-row", at.expect("the command row exists"))
}

fn last_change(rig: &Rig) -> Option<Change> {
    rig.store.changes.lock().expect("changes").last().cloned()
}

#[gpui_kit::test]
fn auto_names_the_tool_it_resolved_to(cx: &mut TestAppContext) {
    let rig = open(cx, Values::default(), &fake());
    let shown = label(cx, &rig, ("settings-tool", AUTO));
    assert!(shown.is_some_and(|text| text.starts_with("Auto") && text.contains("Fake")));
}

#[gpui_kit::test]
fn picking_a_tool_writes_the_setting(cx: &mut TestAppContext) {
    let rig = open(cx, Values::default(), &fake());
    rig.click(cx, ("settings-tool", FAKE));
    assert_eq!(
        last_change(&rig),
        Some(Change::text(Field::GitTool, "fake"))
    );
    rig.press(cx, "right");
    assert_eq!(
        last_change(&rig),
        Some(Change::text(Field::GitTool, "git-cli"))
    );
}

#[gpui_kit::test]
fn only_other_shows_the_command_row(cx: &mut TestAppContext) {
    let rig = open(cx, Values::default(), &fake());
    assert!(label(cx, &rig, command_row()).is_none());
    for _ in 0..OTHER {
        rig.press(cx, "right");
    }
    assert_eq!(
        last_change(&rig),
        Some(Change::text(Field::GitTool, "custom"))
    );
    assert!(label(cx, &rig, command_row()).is_some());
    rig.press(cx, "right");
    assert_eq!(
        last_change(&rig),
        Some(Change::text(Field::GitTool, "none"))
    );
    assert!(label(cx, &rig, command_row()).is_none());
}

#[gpui_kit::test]
fn a_bad_template_shows_the_check_message_under_its_row(cx: &mut TestAppContext) {
    let git = fake();
    git.log.lock().unwrap().refusal = Some("needs a {path} argument".to_owned());
    let values = Values::default()
        .with(&Change::text(Field::GitTool, "custom"))
        .with(&Change::text(Field::GitToolCommand, "tool"));
    let rig = open(cx, values, &git);
    let shown = label(cx, &rig, "settings-git-command-problem");
    assert_eq!(shown.as_deref(), Some("needs a {path} argument"));
    git.log.lock().unwrap().refusal = None;
    assert!(label(cx, &rig, "settings-git-command-problem").is_none());
}

#[gpui_kit::test]
fn opening_settings_refreshes_the_tools_once(cx: &mut TestAppContext) {
    let git = fake();
    let rig = open(cx, Values::default(), &git);
    for _ in 0..3 {
        label(cx, &rig, ("settings-tool", AUTO));
    }
    assert_eq!(git.log.lock().unwrap().refreshes, 1);
}

const NARROW: f32 = 480.;

fn open_sized(cx: &mut TestAppContext, width: f32) -> Rig {
    let rig = rig_git_sized(cx, Values::default(), Arc::new(fake()), px(width));
    let view = rig.view.clone();
    cx.update_window(rig.window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.open_settings_at(Field::GitBadges, window, cx)
        });
    })
    .expect("window");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    rig
}

/// The badges row at `width`: its row, label, title and toggle boxes.
fn badges_row(cx: &mut TestAppContext, width: f32) -> [Bounds<Pixels>; 4] {
    let rig = open_sized(cx, width);
    let at = FIELDS
        .iter()
        .position(|spec| spec.field == Field::GitBadges)
        .expect("row");
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let toggle = window.within(("settings-row", at)).find("settings-toggle");
        [
            window.find(("settings-row", at)).bounds(),
            window.find(("settings-label", at)).bounds(),
            window.find(("settings-title", at)).bounds(),
            toggle.bounds(),
        ]
    })
    .expect("window")
}

#[gpui_kit::test]
fn at_480_px_the_toggle_drops_under_the_title_and_the_label_wraps(cx: &mut TestAppContext) {
    let [row, label, title, toggle] = badges_row(cx, NARROW);
    assert!(toggle.top() >= title.bottom(), "{toggle:?} {title:?}");
    assert!(label.right() <= row.right(), "{label:?} {row:?}");
    assert!(title.right() <= row.right(), "{title:?} {row:?}");
    assert!(
        label.size.height > px(24.),
        "the label did not wrap: {label:?}"
    );
}

#[gpui_kit::test]
fn at_normal_width_the_toggle_stays_beside_the_title(cx: &mut TestAppContext) {
    let [row, label, title, toggle] = badges_row(cx, 1400.);
    assert!(toggle.top() < title.bottom(), "{toggle:?} {title:?}");
    assert!(toggle.left() > title.right(), "{toggle:?} {title:?}");
    assert!(label.size.height < px(24.), "{label:?}");
    assert!(toggle.right() <= row.right());
}
