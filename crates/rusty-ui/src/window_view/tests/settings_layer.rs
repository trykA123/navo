use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};
use rusty_core::{Location, OpKind};
use rusty_fs::{CancelToken, Progress};

use super::app_menu::{Rig, rig};
use super::fkeys::key_labels;
use crate::browser::{JobStatus, JobView};
use crate::settings::FIELDS;
use crate::tokens::ThemeChoice;

fn open_settings(cx: &mut TestAppContext, rig: &Rig) {
    rig.press(cx, "ctrl-,");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
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
            cancel: CancelToken::new(),
            source_tab,
        });
        cx.notify();
    });
}

fn row_under_the_job_card(cx: &mut TestAppContext, rig: &Rig) -> Option<usize> {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let card = window.find("job-card").bounds();
        (0..FIELDS.len()).find(|index| {
            window
                .try_find(("settings-row", *index))
                .is_some_and(|row| card.contains(&row.bounds().center()))
        })
    })
    .expect("window")
}

#[gpui_kit::test]
fn the_job_card_stays_under_the_settings_sheet(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "tab");
    open_settings(cx, &rig);
    add_running_copy(cx, &rig);
    let row = row_under_the_job_card(cx, &rig).expect("a settings row overlaps the job card");
    rig.click(cx, ("settings-row", row));
    assert_eq!(rig.view.read_with(cx, |view, _| view.settings_row()), row);
}

#[gpui_kit::test]
fn open_settings_switch_the_f_keys_to_the_settings_layer(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
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
    rig.press(cx, "escape");
    assert_eq!(key_labels(cx, rig.window).0, base);
}
