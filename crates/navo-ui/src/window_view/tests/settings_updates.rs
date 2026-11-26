use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, SharedString, TestAppContext, VisualTestContext, px, size};

use super::app_menu::{Rig, rig_updates};
use crate::settings::{Change, Field, Section};
use crate::tokens::{self, Mode, ThemeChoice};
use crate::window_view::{
    RollbackCompatibilityView, RollbackTargetView, UpdateFailure, UpdateHooks, UpdateView,
};

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

fn has(cx: &mut TestAppContext, rig: &Rig, id: impl Into<gpui_kit::ElementId>) -> bool {
    let id = id.into();
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
            "Navo 0.4.0 is ready.",
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
fn a_copy_that_cannot_update_says_why_and_offers_the_installer(cx: &mut TestAppContext) {
    let rig = rig_updates(cx, UpdateHooks::none());
    open_help(cx, &rig);
    assert_eq!(
        status(cx, &rig).as_deref(),
        Some("This copy does not update itself.")
    );
    assert!(!has(cx, &rig, "settings-update-check"));
    assert!(has(cx, &rig, "settings-update-installer"));
    assert!(has(cx, &rig, "settings-update-auto-unavailable"));
}

#[gpui_kit::test]
fn the_installer_button_opens_the_latest_release(cx: &mut TestAppContext) {
    let rig = rig_updates(cx, UpdateHooks::none());
    open_help(cx, &rig);
    rig.click(cx, "settings-update-installer");
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://github.com/trykA123/navo/releases/latest")
    );
}

#[gpui_kit::test]
fn the_toggle_is_off_on_a_copy_that_cannot_update(cx: &mut TestAppContext) {
    let rig = rig_updates(cx, UpdateHooks::none());
    open_help(cx, &rig);
    rig.click(cx, "settings-toggle");
    rig.press(cx, "space");
    let stored = rig
        .store
        .stored
        .lock()
        .expect("stored")
        .flag(Field::CheckUpdates);
    assert!(stored);
}

#[gpui_kit::test]
fn the_ready_notice_shows_once_per_version_and_goes_on_the_next_command(cx: &mut TestAppContext) {
    let (mut updates, _, _) = hooks(UpdateView::UpToDate);
    let versions = Arc::new(Mutex::new(vec![SharedString::from("0.4.0")]));
    let queue = versions.clone();
    updates.take_ready_notice = Arc::new(move || queue.lock().expect("queue").pop());
    let rig = rig_updates(cx, updates);
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(6));
    cx.run_until_parked();
    let text = rig.view.read_with(cx, |view, _| {
        view.update_notice().map(|notice| notice.text.to_string())
    });
    assert_eq!(
        text.as_deref(),
        Some("Navo 0.4.0 is ready. It updates when you next start Navo.")
    );
    assert!(versions.lock().expect("queue").is_empty());
    rig.press(cx, "down");
    let gone = rig.view.read_with(cx, |view, _| view.update_notice());
    assert_eq!(gone, None);
}

#[gpui_kit::test]
fn ready_lines_of_an_installed_copy_offer_restart_whatever_the_tray_setting(
    cx: &mut TestAppContext,
) {
    let (mut updates, _, _) = hooks(UpdateView::Ready {
        version: "0.4.0".into(),
    });
    let notice = Arc::new(Mutex::new(Some(SharedString::from("0.4.0"))));
    updates.take_ready_notice = Arc::new(move || notice.lock().expect("notice").take());
    let rig = rig_updates(cx, updates);
    open_help(cx, &rig);
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(6));
    cx.run_until_parked();
    for keep in [true, false] {
        rig.view.update(cx, |view, cx| {
            view.values = view.values.with(&crate::settings::Change {
                field: Field::KeepInTray,
                value: crate::settings::Value::Flag(keep),
            });
            cx.notify();
        });
        assert_eq!(status(cx, &rig).as_deref(), Some("Navo 0.4.0 is ready."));
        let ready = rig.view.read_with(cx, |view, _| {
            view.update_notice().expect("ready").text.to_string()
        });
        assert_eq!(ready, "Navo 0.4.0 is ready.");
    }
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
    let (updates, _, _) = hooks(UpdateView::UpToDate);
    let rig = rig_updates(cx, updates);
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
    let (updates, _, _) = hooks(UpdateView::UpToDate);
    let rig = rig_updates(cx, updates);
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

#[gpui_kit::test]
fn downloading_renders_progress_bar_and_failed_offers_retry_and_manual_download(
    cx: &mut TestAppContext,
) {
    let (downloading, _, _) = hooks(UpdateView::Downloading {
        version: "0.9.0".into(),
        percent: 42,
    });
    let rig = rig_updates(cx, downloading);
    open_help(cx, &rig);
    assert!(has(cx, &rig, "settings-update-progress"));

    let (failed, _, _) = hooks(UpdateView::Failed(UpdateFailure::Offline));
    let rig = rig_updates(cx, failed);
    open_help(cx, &rig);
    assert!(has(cx, &rig, "settings-update-check"));
    assert!(has(cx, &rig, "settings-update-installer"));
}

fn label(cx: &mut TestAppContext, rig: &Rig, id: impl Into<gpui_kit::ElementId>) -> Option<String> {
    let id = id.into();
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .and_then(|found| found.label().map(str::to_owned))
    })
    .expect("window")
}

#[gpui_kit::test]
fn title_update_pill_survives_user_commands_and_dismisses(cx: &mut TestAppContext) {
    let restarts = Arc::new(AtomicUsize::new(0));
    let restart_calls = restarts.clone();
    let (mut ready, _, _) = hooks(UpdateView::Ready {
        version: "0.9.0".into(),
    });
    ready.restart = Arc::new(move || {
        restart_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    });
    let rig = rig_updates(cx, ready);
    assert_eq!(
        label(cx, &rig, "title-update-pill").as_deref(),
        Some("Update 0.9.0")
    );
    rig.press(cx, "down");
    assert!(has(cx, &rig, "title-update-pill"));
    rig.click(cx, "title-update-pill-dismiss");
    assert!(!has(cx, &rig, "title-update-pill"));
    assert_eq!(restarts.load(Ordering::SeqCst), 0);
    assert!(!has(cx, &rig, "settings-update-card"));

    for quiet in [
        UpdateView::Hidden,
        UpdateView::Off,
        UpdateView::Idle,
        UpdateView::Checking,
        UpdateView::UpToDate,
    ] {
        let (quiet_hooks, _, _) = hooks(quiet);
        let rig = rig_updates(cx, quiet_hooks);
        assert!(!has(cx, &rig, "title-update-pill"));
    }
}

#[gpui_kit::test]
fn title_update_pill_actions_and_state_transitions(cx: &mut TestAppContext) {
    let (downloading, state, _) = hooks(UpdateView::Downloading {
        version: "0.9.0".into(),
        percent: 42,
    });
    let rig = rig_updates(cx, downloading);
    assert_eq!(
        label(cx, &rig, "title-update-pill").as_deref(),
        Some("Updating 42%")
    );
    rig.click(cx, "title-update-pill-dismiss");
    assert!(!has(cx, &rig, "title-update-pill"));
    assert!(!has(cx, &rig, "settings-update-card"));

    *state.lock().expect("state") = UpdateView::Downloading {
        version: "0.9.0".into(),
        percent: 80,
    };
    rig.view.update(cx, |view, cx| view.refresh_updates(cx));
    assert!(!has(cx, &rig, "title-update-pill"));

    *state.lock().expect("state") = UpdateView::Ready {
        version: "0.9.0".into(),
    };
    rig.view.update(cx, |view, cx| view.refresh_updates(cx));
    assert_eq!(
        label(cx, &rig, "title-update-pill").as_deref(),
        Some("Update 0.9.0")
    );

    let (downloading_open, _, _) = hooks(UpdateView::Downloading {
        version: "0.9.0".into(),
        percent: 42,
    });
    let rig = rig_updates(cx, downloading_open);
    rig.click(cx, "title-update-pill");
    assert!(has(cx, &rig, "settings-update-card"));

    let (failed, _, _) = hooks(UpdateView::Failed(UpdateFailure::Offline));
    let rig = rig_updates(cx, failed);
    assert_eq!(
        label(cx, &rig, "title-update-pill").as_deref(),
        Some("Update failed")
    );
    rig.click(cx, "title-update-pill");
    assert!(has(cx, &rig, "settings-update-card"));

    let restarts = Arc::new(AtomicUsize::new(0));
    let restart_calls = restarts.clone();
    let (mut ready, _, _) = hooks(UpdateView::Ready {
        version: "0.9.0".into(),
    });
    ready.restart = Arc::new(move || {
        restart_calls.fetch_add(1, Ordering::SeqCst);
        Err(UpdateFailure::Apply)
    });
    let rig = rig_updates(cx, ready);
    rig.click(cx, "title-update-pill");
    cx.run_until_parked();
    assert_eq!(restarts.load(Ordering::SeqCst), 1);
    assert_eq!(
        label(cx, &rig, "title-update-pill").as_deref(),
        Some("Update failed")
    );
}

#[gpui_kit::test]
fn title_update_pill_keeps_polling_downloading_after_settings_closes(cx: &mut TestAppContext) {
    let (downloading, state, _) = hooks(UpdateView::Downloading {
        version: "0.9.0".into(),
        percent: 10,
    });
    let rig = rig_updates(cx, downloading);
    open_help(cx, &rig);
    rig.press(cx, "escape");
    assert!(!rig.view.read_with(cx, |view, _| view.settings.is_open()));

    *state.lock().expect("state") = UpdateView::Downloading {
        version: "0.9.0".into(),
        percent: 55,
    };
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert!(!has(cx, &rig, "settings-update-card"));
    assert_eq!(
        label(cx, &rig, "title-update-pill").as_deref(),
        Some("Updating 55%")
    );

    *state.lock().expect("state") = UpdateView::Ready {
        version: "0.9.0".into(),
    };
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(
        label(cx, &rig, "title-update-pill").as_deref(),
        Some("Update 0.9.0")
    );
}

fn bounds_of(
    cx: &mut TestAppContext,
    rig: &Rig,
    id: impl Into<gpui_kit::ElementId>,
) -> Option<gpui_kit::Bounds<gpui_kit::Pixels>> {
    let id = id.into();
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.try_find(id).map(|found| found.bounds())
    })
    .expect("window")
}

#[gpui_kit::test]
fn skipped_version_status_renders_and_clears_on_click(cx: &mut TestAppContext) {
    let skipped = Arc::new(Mutex::new(Some(SharedString::from("0.9.3"))));
    let cleared = Arc::new(AtomicUsize::new(0));
    let (read, clear_count, store) = (skipped.clone(), cleared.clone(), skipped.clone());
    let (mut updates, _, _) = hooks(UpdateView::UpToDate);
    updates.skipped_version = Arc::new(move || read.lock().expect("skipped").clone());
    updates.clear_skipped_version = Arc::new(move || {
        clear_count.fetch_add(1, Ordering::SeqCst);
        *store.lock().expect("skipped") = None;
    });
    let rig = rig_updates(cx, updates);
    open_help(cx, &rig);

    assert_eq!(
        label(cx, &rig, "settings-update-skipped").as_deref(),
        Some("Skipped v0.9.3 after rollback — newer releases still offered")
    );
    assert!(has(cx, &rig, "settings-update-clear-skip"));

    rig.click(cx, "settings-update-clear-skip");
    assert_eq!(cleared.load(Ordering::SeqCst), 1);
    assert!(skipped.lock().expect("skipped").is_none());
    assert!(!has(cx, &rig, "settings-update-skipped"));
    assert!(!has(cx, &rig, "settings-update-clear-skip"));
}

#[gpui_kit::test]
fn compatible_rollback_target_requires_two_step_confirmation(cx: &mut TestAppContext) {
    let applied: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let skipped = Arc::new(Mutex::new(None::<SharedString>));
    let targets = vec![
        RollbackTargetView {
            version: "0.9.2".into(),
            status: RollbackCompatibilityView::Compatible,
        },
        RollbackTargetView {
            version: "0.9.1".into(),
            status: RollbackCompatibilityView::CompatibleWithBackup,
        },
    ];
    let (applied_write, skipped_read, skipped_write) =
        (applied.clone(), skipped.clone(), skipped.clone());
    let (mut updates, _, _) = hooks(UpdateView::UpToDate);
    updates.rollback_targets = Arc::new(move || targets.clone());
    updates.skipped_version = Arc::new(move || skipped_read.lock().expect("skipped").clone());
    updates.apply_rollback = Arc::new(move |version| {
        applied_write
            .lock()
            .expect("applied")
            .push(version.to_owned());
        *skipped_write.lock().expect("skipped") = Some("0.9.3".into());
        Ok(())
    });
    let rig = rig_updates(cx, updates);
    open_help(cx, &rig);

    // Collapsed by default until user expands "Previous versions".
    assert!(has(cx, &rig, "settings-update-rollback-fold"));
    assert!(!has(cx, &rig, ("settings-update-rollback", 0usize)));

    rig.click(cx, "settings-update-rollback-fold");
    assert!(has(cx, &rig, ("settings-update-rollback", 0usize)));
    assert!(has(cx, &rig, ("settings-update-rollback", 1usize)));
    assert_eq!(
        label(cx, &rig, ("settings-update-rollback-reason", 1usize)).as_deref(),
        Some("Restores pre-update backup automatically")
    );

    // First click opens inline confirmation and does NOT invoke rollback yet.
    rig.click(cx, ("settings-update-rollback", 0usize));
    assert!(applied.lock().expect("applied").is_empty());
    assert_eq!(
        label(cx, &rig, ("settings-update-rollback-confirm-text", 0usize)).as_deref(),
        Some("Confirm rollback to v0.9.2?")
    );
    assert!(has(cx, &rig, ("settings-update-rollback-confirm", 0usize)));
    assert!(has(cx, &rig, ("settings-update-rollback-cancel", 0usize)));

    // Cancel dismisses confirmation without invoking rollback.
    rig.click(cx, ("settings-update-rollback-cancel", 0usize));
    assert!(applied.lock().expect("applied").is_empty());
    assert!(!has(cx, &rig, ("settings-update-rollback-confirm", 0usize)));

    // Re-open confirmation and confirm invokes rollback.
    rig.click(cx, ("settings-update-rollback", 0usize));
    rig.click(cx, ("settings-update-rollback-confirm", 0usize));
    assert_eq!(applied.lock().expect("applied").as_slice(), ["0.9.2"]);
    assert!(has(cx, &rig, "settings-update-skipped"));
}

#[gpui_kit::test]
fn incompatible_rollback_target_disables_button_and_shows_reason(cx: &mut TestAppContext) {
    let applied = Arc::new(AtomicUsize::new(0));
    let reason = "Database schema v8 requires a pre-update backup (none found).";
    let targets = vec![RollbackTargetView {
        version: "0.8.9".into(),
        status: RollbackCompatibilityView::Incompatible {
            reason: reason.into(),
        },
    }];
    let applied_count = applied.clone();
    let (mut updates, _, _) = hooks(UpdateView::UpToDate);
    updates.rollback_targets = Arc::new(move || targets.clone());
    updates.apply_rollback = Arc::new(move |_| {
        applied_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    });
    let rig = rig_updates(cx, updates);
    open_help(cx, &rig);
    rig.click(cx, "settings-update-rollback-fold");

    assert_eq!(
        label(cx, &rig, ("settings-update-rollback-reason", 0usize)).as_deref(),
        Some(reason)
    );
    assert!(has(cx, &rig, ("settings-update-rollback", 0usize)));
    rig.click(cx, ("settings-update-rollback", 0usize));
    assert_eq!(applied.load(Ordering::SeqCst), 0);
    assert!(!has(cx, &rig, ("settings-update-rollback-confirm", 0usize)));
}

#[gpui_kit::test]
fn rollback_section_renders_in_dark_and_light_at_desktop_and_390px(cx: &mut TestAppContext) {
    let targets = vec![
        RollbackTargetView {
            version: "0.9.2".into(),
            status: RollbackCompatibilityView::Compatible,
        },
        RollbackTargetView {
            version: "0.9.1".into(),
            status: RollbackCompatibilityView::CompatibleWithBackup,
        },
        RollbackTargetView {
            version: "0.8.9".into(),
            status: RollbackCompatibilityView::Incompatible {
                reason: "Session format v7 is newer than this release.".into(),
            },
        },
    ];
    let (mut updates, _, _) = hooks(UpdateView::UpToDate);
    updates.skipped_version = Arc::new(|| Some("0.9.3".into()));
    updates.rollback_targets = Arc::new(move || targets.clone());
    let rig = rig_updates(cx, updates);
    open_help(cx, &rig);
    rig.click(cx, "settings-update-rollback-fold");
    rig.click(cx, ("settings-update-rollback", 0usize));

    for (choice, mode) in [
        (ThemeChoice::Dark, Mode::Dark),
        (ThemeChoice::Light, Mode::Light),
    ] {
        cx.update_window(rig.window, |_, window, cx| {
            rig.view.update(cx, |view, cx| {
                view.change_setting(Change::theme(choice), window, cx);
            });
        })
        .expect("window");
        assert_eq!(cx.read(|cx| tokens::current(cx).mode), mode);

        for width in [1440., 390.] {
            VisualTestContext::from_window(rig.window, cx)
                .simulate_resize(size(px(width), px(900.)));
            let card = bounds_of(cx, &rig, "settings-update-card").expect("update card");
            let skipped = bounds_of(cx, &rig, "settings-update-skipped").expect("skipped");
            let clear = bounds_of(cx, &rig, "settings-update-clear-skip").expect("clear skip");
            let fold = bounds_of(cx, &rig, "settings-update-rollback-fold").expect("fold");
            let confirm =
                bounds_of(cx, &rig, ("settings-update-rollback-confirm", 0usize)).expect("confirm");
            let cancel =
                bounds_of(cx, &rig, ("settings-update-rollback-cancel", 0usize)).expect("cancel");
            let reason =
                bounds_of(cx, &rig, ("settings-update-rollback-reason", 2usize)).expect("reason");

            for (name, box_bounds) in [
                ("skipped", skipped),
                ("clear", clear),
                ("fold", fold),
                ("confirm", confirm),
                ("cancel", cancel),
                ("reason", reason),
            ] {
                assert!(
                    box_bounds.size.width > px(0.) && box_bounds.size.height > px(0.),
                    "{name} has zero size at {width}px in {mode:?}: {box_bounds:?}"
                );
                assert!(
                    box_bounds.left() >= card.left() - px(1.)
                        && box_bounds.right() <= card.right() + px(1.),
                    "{name} overflows card horizontally at {width}px in {mode:?}: {box_bounds:?} vs {card:?}"
                );
            }
        }
    }
}
