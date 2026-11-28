use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext, px, size};
use navo_core::{Location, OpKind};
use navo_fs::{CancelToken, Progress};

use super::app_menu::{Rig, Target, rig, rig_on};
use super::fkeys::key_labels;
use super::settings_sheet::sheet_has_focus;
use super::with_fkey_bar;
use crate::browser::{JobStatus, JobView, RenameReport};
use crate::settings::Values;
use crate::tokens::ThemeChoice;

fn open_settings(cx: &mut TestAppContext, rig: &Rig) {
    rig.press(cx, "ctrl-,");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_workspaces_menu_entry_leaves_focus_alone_while_settings_is_open(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_settings(cx, &rig);
    rig.press(cx, "f10");
    rig.press(cx, "w");
    rig.press(cx, "enter");
    assert!(sheet_has_focus(cx, &rig));
    assert!(!rig.view.read_with(cx, |view, _| view.workspace_keys.pinned));
}

fn add_running_copy(cx: &mut TestAppContext, rig: &Rig) {
    let at = |path: &str| Location::new(path).expect("location");
    rig.view.update(cx, |view, cx| {
        let source_tab = view.browser.state().active_tab().id();
        view.browser.jobs_mut().push(JobView {
            id: 1,
            kind: OpKind::Copy,
            from: Some(at("/home")),
            target: Some(at("/archive")),
            sources: 4,
            progress: Progress {
                bytes_done: 10,
                bytes_total: 100,
                files_done: 1,
                files_total: 4,
                current: Some(at("/home/a.txt")),
                paused: false,
            },
            status: JobStatus::Running,
            is_waiting: false,
            cancel: CancelToken::new(),
            source_tab,
            undo: false,
            report: RenameReport::default(),
            first_new: None,
            refused: Vec::new(),
        });
        view.refresh_activity(cx);
    });
}

#[gpui_kit::test]
fn the_job_card_stays_in_navo_while_settings_rows_live_in_the_settings_window(
    cx: &mut TestAppContext,
) {
    let rig = rig(cx, ThemeChoice::System);
    VisualTestContext::from_window(rig.window, cx).simulate_resize(size(px(1000.), px(640.)));
    rig.press(cx, "tab");
    open_settings(cx, &rig);
    add_running_copy(cx, &rig);
    rig.press(cx, "ctrl-j");
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("job-card").is_some());
        assert!(window.try_find("settings-rows").is_none());
    })
    .expect("navo window");
    rig.click_in(cx, Target::Settings, ("settings-row", 1usize));
    assert_eq!(rig.view.read_with(cx, |view, _| view.settings_row()), 1);
}

#[gpui_kit::test]
fn open_settings_switch_the_f_keys_to_the_settings_layer(cx: &mut TestAppContext) {
    let rig = rig_on(cx, with_fkey_bar(Values::default()), None);
    let (base, _) = key_labels(cx, rig.window);
    assert_eq!(base[1], "F2 Rename");
    open_settings(cx, &rig);
    let (settings, _) = key_labels(cx, rig.window);
    assert_eq!(
        settings,
        [
            "F1",
            "F2 Edit value",
            "F3 TOML line",
            "F4",
            "F5",
            "F6",
            "F7",
            "F8",
            "F9",
            "F10 Menu"
        ]
    );
    rig.press_in(cx, Target::Settings, "escape");
    assert_eq!(key_labels(cx, rig.window).0, base);
}
