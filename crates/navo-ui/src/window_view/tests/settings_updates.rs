use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

use super::app_menu::{Rig, rig_updates};
use crate::settings::{Field, Section};
use crate::window_view::{UpdateFailure, UpdateHooks, UpdateView};

type Shared = Arc<Mutex<UpdateView>>;

fn hooks(start: UpdateView) -> (UpdateHooks, Shared, Arc<AtomicUsize>) {
    let state: Shared = Arc::new(Mutex::new(start));
    let calls = Arc::new(AtomicUsize::new(0));
    let (read, count) = (state.clone(), calls.clone());
    let hooks = UpdateHooks {
        state: Arc::new(move || read.lock().expect("state").clone()),
        check_now: Arc::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
        }),
        ..UpdateHooks::none()
    };
    (hooks, state, calls)
}

fn open_help(cx: &mut TestAppContext, rig: &Rig) {
    rig.press(cx, "ctrl-,");
    rig.click(cx, ("settings-section", Section::ALL.len() - 1));
}

fn status(cx: &mut TestAppContext, rig: &Rig) -> Option<String> {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find("settings-update-status")
            .and_then(|found| found.label().map(str::to_owned))
    })
    .expect("window")
}

fn has(cx: &mut TestAppContext, rig: &Rig, id: &'static str) -> bool {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some()
    })
    .expect("window")
}

fn cases() -> Vec<(UpdateView, &'static str)> {
    let limited = UpdateFailure::RateLimited {
        next_try: "14:30".into(),
    };
    vec![
        (UpdateView::Off, "Automatic checks are off."),
        (UpdateView::Idle, "Navo has not checked for updates yet."),
        (UpdateView::Checking, "Checking for updates…"),
        (UpdateView::UpToDate, "Navo is up to date"),
        (
            UpdateView::Downloading {
                version: "0.4.0".into(),
                percent: 40,
            },
            "Downloading Navo 0.4.0 (40 %)",
        ),
        (
            UpdateView::Ready {
                version: "0.4.0".into(),
            },
            "Navo 0.4.0 is ready. It updates when you next start Navo.",
        ),
        (
            UpdateView::Failed(UpdateFailure::Offline),
            "Could not reach GitHub",
        ),
        (
            UpdateView::Failed(limited),
            "GitHub asks Navo to wait. Next try after 14:30",
        ),
        (
            UpdateView::Failed(UpdateFailure::BadFeed),
            "The update list could not be read",
        ),
        (
            UpdateView::Failed(UpdateFailure::Interrupted),
            "The download stopped. It starts again next time",
        ),
        (
            UpdateView::Failed(UpdateFailure::Damaged),
            "The download was damaged and was removed",
        ),
        (
            UpdateView::Failed(UpdateFailure::Busy),
            "Another Navo is updating",
        ),
    ]
}

#[gpui_kit::test]
fn every_state_has_its_line(cx: &mut TestAppContext) {
    for (view, text) in cases() {
        let (updates, _, _) = hooks(view);
        let rig = rig_updates(cx, updates);
        open_help(cx, &rig);
        let shown = status(cx, &rig);
        assert_eq!(shown.as_deref(), Some(text));
        assert!(has(cx, &rig, "settings-update-check"));
    }
}

#[gpui_kit::test]
fn a_copy_that_cannot_update_says_why_and_has_no_button(cx: &mut TestAppContext) {
    let rig = rig_updates(cx, UpdateHooks::none());
    open_help(cx, &rig);
    assert_eq!(
        status(cx, &rig).as_deref(),
        Some("This copy does not update itself. Get the installer from GitHub.")
    );
    assert!(!has(cx, &rig, "settings-update-check"));
}

#[gpui_kit::test]
fn check_now_calls_the_hook_and_is_off_while_busy(cx: &mut TestAppContext) {
    let (updates, state, calls) = hooks(UpdateView::UpToDate);
    let rig = rig_updates(cx, updates);
    open_help(cx, &rig);
    rig.click(cx, "settings-update-check");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    *state.lock().expect("state") = UpdateView::Checking;
    rig.view.update(cx, |view, cx| view.refresh_updates(cx));
    assert_eq!(status(cx, &rig).as_deref(), Some("Checking for updates…"));
    rig.click(cx, "settings-update-check");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[gpui_kit::test]
fn the_toggle_writes_updates_check(cx: &mut TestAppContext) {
    let rig = rig_updates(cx, UpdateHooks::none());
    open_help(cx, &rig);
    let on = rig
        .view
        .read_with(cx, |view, _| view.values.flag(Field::CheckUpdates));
    assert!(on);
    rig.click(cx, "settings-toggle");
    let stored = rig
        .store
        .stored
        .lock()
        .expect("stored")
        .flag(Field::CheckUpdates);
    assert!(!stored);
    assert_eq!(Field::CheckUpdates.spec().key, "updates.check");
}

#[gpui_kit::test]
fn the_toggle_is_reached_by_keys_on_the_help_page(cx: &mut TestAppContext) {
    let rig = rig_updates(cx, UpdateHooks::none());
    open_help(cx, &rig);
    rig.press(cx, "space");
    let stored = rig
        .store
        .stored
        .lock()
        .expect("stored")
        .flag(Field::CheckUpdates);
    assert!(!stored);
    rig.press(cx, "right");
    let stored = rig
        .store
        .stored
        .lock()
        .expect("stored")
        .flag(Field::CheckUpdates);
    assert!(stored);
}

#[test]
fn search_finds_the_help_page_by_updates() {
    for word in ["updates", "update"] {
        let filter = crate::settings::Filter::new(word);
        assert_eq!(
            filter.first_in(Section::Help),
            Some(crate::settings::HELP_ROW)
        );
    }
}
