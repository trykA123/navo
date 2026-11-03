use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt;

use super::super::tests::{
    fake, open_options, options_for, press, two_panes, type_in, viewer_window,
};
use super::*;
use crate::window_view::{UpdateFailure, UpdateHooks, UpdateView};

fn ready() -> UpdateView {
    UpdateView::Ready {
        version: "0.4.0".into(),
    }
}

#[gpui_kit::test]
fn restart_saves_before_apply_and_failure_keeps_the_window(cx: &mut TestAppContext) {
    let fs = fake();
    let mut options = options_for(fs.clone(), two_panes(&fs));
    options.app_menu.identity.install = crate::app_menu::Install::Installed;
    let calls = Arc::new(Mutex::new(Vec::new()));
    let saves = calls.clone();
    options.on_save = Rc::new(move |_| saves.lock().unwrap().push("save"));
    let applies = calls.clone();
    options.settings.updates = UpdateHooks {
        state: Arc::new(ready),
        restart: Arc::new(move || {
            applies.lock().unwrap().push("apply");
            Err(UpdateFailure::Busy)
        }),
        ..UpdateHooks::none()
    };
    let (window, view) = open_options(cx, options);
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| view.request_restart(window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(*calls.lock().unwrap(), ["save", "apply"]);
    assert!(cx.update_window(window, |_, _, _| ()).is_ok());
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.settings.updates.view,
            UpdateView::Failed(UpdateFailure::Busy)
        );
    });
}

#[gpui_kit::test]
fn restart_availability_controls_the_palette(cx: &mut TestAppContext) {
    let fs = fake();
    let mut options = options_for(fs.clone(), two_panes(&fs));
    options.app_menu.identity.install = crate::app_menu::Install::Installed;
    let state = Arc::new(Mutex::new(UpdateView::Hidden));
    let read = state.clone();
    options.settings.updates.state = Arc::new(move || read.lock().unwrap().clone());
    let (_, view) = open_options(cx, options);
    for status in [
        UpdateView::Hidden,
        UpdateView::Idle,
        UpdateView::Checking,
        ready(),
    ] {
        *state.lock().unwrap() = status.clone();
        view.update(cx, |view, cx| view.refresh_updates(cx));
        view.read_with(cx, |view, _| {
            assert_eq!(view.restart_available(), status == ready());
            assert_eq!(
                view.palette_entries()
                    .iter()
                    .any(|e| e.spec.name == "app.restart_update"),
                status == ready()
            );
        });
    }
    view.update(cx, |view, _| {
        view.identity.install = crate::app_menu::Install::Portable
    });
    assert!(!view.read_with(cx, |view, _| view.restart_available()));
    assert!(!view.read_with(cx, |view, _| {
        view.palette_entries()
            .iter()
            .any(|e| e.spec.name == "app.restart_update")
    }));
}

fn add_job(view: &Entity<NavoWindow>, cx: &mut TestAppContext, id: u64) -> navo_fs::CancelToken {
    let cancel = navo_fs::CancelToken::new();
    view.update(cx, |view, cx| {
        let source_tab = view.browser.state().active_tab().id();
        view.browser.jobs_mut().push(crate::browser::JobView {
            id,
            kind: navo_core::OpKind::Copy,
            from: None,
            target: None,
            sources: 1,
            progress: navo_fs::Progress {
                bytes_done: 0,
                bytes_total: 0,
                files_done: 0,
                files_total: 1,
                current: None,
                paused: false,
            },
            status: crate::browser::JobStatus::Running,
            is_waiting: false,
            cancel: cancel.clone(),
            source_tab,
            undo: false,
            report: crate::browser::RenameReport::default(),
            first_new: None,
            refused: Vec::new(),
        });
        cx.notify();
    });
    cancel
}

fn frame(window: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
}

fn request(window: AnyWindowHandle, view: &Entity<NavoWindow>, cx: &mut TestAppContext) {
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| view.request_restart(window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn job_done(view: &Entity<NavoWindow>, cx: &mut TestAppContext, id: u64) {
    view.update(cx, |view, cx| {
        view.browser
            .jobs_mut()
            .iter_mut()
            .find(|job| job.id == id)
            .unwrap()
            .status = crate::browser::JobStatus::Done;
        view.wake(cx);
        cx.notify();
    });
}

fn held_window(
    cx: &mut TestAppContext,
) -> (
    AnyWindowHandle,
    Entity<NavoWindow>,
    Arc<Mutex<Vec<&'static str>>>,
) {
    let fs = fake();
    let mut options = options_for(fs.clone(), two_panes(&fs));
    options.app_menu.identity.install = crate::app_menu::Install::Installed;
    let calls = Arc::new(Mutex::new(Vec::new()));
    let saves = calls.clone();
    options.on_save = Rc::new(move |_| saves.lock().unwrap().push("save"));
    let applies = calls.clone();
    options.settings.updates = UpdateHooks {
        state: Arc::new(ready),
        restart: Arc::new(move || {
            applies.lock().unwrap().push("apply");
            Err(UpdateFailure::Busy)
        }),
        ..UpdateHooks::none()
    };
    let (window, view) = open_options(cx, options);
    (window, view, calls)
}

#[gpui_kit::test]
fn restart_waits_for_all_jobs_including_new_jobs_without_cancelling(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt;
    let (window, view, calls) = held_window(cx);
    let first = add_job(&view, cx, 1);
    request(window, &view, cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.settings.updates.restart),
        Restart::Asking
    );
    cx.update_window(window, |_, window, cx| window.press("enter", cx))
        .unwrap();
    frame(window, cx);
    assert!(!first.is_cancelled());
    assert!(calls.lock().unwrap().is_empty());
    let second = add_job(&view, cx, 2);
    job_done(&view, cx, 1);
    frame(window, cx);
    assert!(calls.lock().unwrap().is_empty());
    assert!(!second.is_cancelled());
    job_done(&view, cx, 2);
    frame(window, cx);
    cx.run_until_parked();
    assert_eq!(*calls.lock().unwrap(), ["save", "apply"]);
    assert!(!first.is_cancelled());
    assert!(!second.is_cancelled());
}

#[gpui_kit::test]
fn restart_dialog_escape_and_status_cancel_clear_the_wait(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt;
    let (window, view, calls) = held_window(cx);
    let job = add_job(&view, cx, 1);
    request(window, &view, cx);
    cx.update_window(window, |_, window, cx| window.press("escape", cx))
        .unwrap();
    frame(window, cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.settings.updates.restart),
        Restart::Idle
    );
    request(window, &view, cx);
    cx.update_window(window, |_, window, cx| window.press("enter", cx))
        .unwrap();
    frame(window, cx);
    cx.update_window(window, |_, window, cx| window.click("restart-cancel", cx))
        .unwrap();
    frame(window, cx);
    job_done(&view, cx, 1);
    frame(window, cx);
    assert!(calls.lock().unwrap().is_empty());
    assert!(!job.is_cancelled());
}

#[gpui_kit::test]
fn restart_success_closes_only_after_apply(cx: &mut TestAppContext) {
    let fs = fake();
    let mut options = options_for(fs.clone(), two_panes(&fs));
    options.app_menu.identity.install = crate::app_menu::Install::Installed;
    let calls = Arc::new(Mutex::new(Vec::new()));
    let saves = calls.clone();
    options.on_save = Rc::new(move |_| saves.lock().unwrap().push("save"));
    let applies = calls.clone();
    options.settings.updates = UpdateHooks {
        state: Arc::new(ready),
        restart: Arc::new(move || {
            applies.lock().unwrap().push("apply");
            Ok(())
        }),
        ..UpdateHooks::none()
    };
    let (window, view) = open_options(cx, options);
    request(window, &view, cx);
    cx.run_until_parked();
    assert_eq!(*calls.lock().unwrap(), ["save", "apply"]);
    assert!(cx.update_window(window, |_, _, _| ()).is_err());
}

#[gpui_kit::test]
fn restart_buttons_offer_the_same_handoff_in_help_and_the_ready_notice(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt;
    let (window, view, calls) = held_window(cx);
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.open_settings_at(crate::settings::Field::CheckUpdates, window, cx)
        });
        window.render_frame(cx);
        assert!(window.try_find("settings-update-restart").is_some());
        window.click("settings-update-restart", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(*calls.lock().unwrap(), ["save", "apply"]);
}

fn restart_state(view: &Entity<NavoWindow>, cx: &mut TestAppContext) -> Restart {
    view.read_with(cx, |view, _| view.settings.updates.restart)
}

#[gpui_kit::test]
fn a_restore_in_progress_holds_the_restart_until_it_ends(cx: &mut TestAppContext) {
    let (window, view, calls) = held_window(cx);
    view.update(cx, |view, _| view.job_ui.is_restoring = true);
    request(window, &view, cx);
    assert_eq!(restart_state(&view, cx), Restart::Asking);
    assert_eq!(view.read_with(cx, |view, _| view.restart_job_count()), 1);
    assert!(calls.lock().unwrap().is_empty());
    view.update(cx, |view, cx| {
        view.job_ui.is_restoring = false;
        cx.notify();
    });
    frame(window, cx);
    assert_eq!(*calls.lock().unwrap(), ["save", "apply"]);
}

#[gpui_kit::test]
fn the_jobs_dialog_closes_and_restarts_when_the_last_job_ends(cx: &mut TestAppContext) {
    let (window, view, calls) = held_window(cx);
    let job = add_job(&view, cx, 1);
    request(window, &view, cx);
    assert_eq!(restart_state(&view, cx), Restart::Asking);
    job_done(&view, cx, 1);
    frame(window, cx);
    assert_eq!(*calls.lock().unwrap(), ["save", "apply"]);
    assert!(
        cx.update_window(window, |_, window, _| window
            .try_find("restart-dialog")
            .is_none())
            .unwrap()
    );
    assert!(!job.is_cancelled());
}

#[gpui_kit::test]
fn a_dropped_restart_says_why_in_the_status_bar(cx: &mut TestAppContext) {
    let (window, view, calls) = held_window(cx);
    add_job(&view, cx, 1);
    request(window, &view, cx);
    cx.update_window(window, |_, window, cx| window.press("enter", cx))
        .unwrap();
    frame(window, cx);
    assert_eq!(restart_state(&view, cx), Restart::Waiting);
    view.update(cx, |view, _| view.saver.error = Some("disk full".into()));
    job_done(&view, cx, 1);
    frame(window, cx);
    assert_eq!(restart_state(&view, cx), Restart::Idle);
    assert_eq!(*calls.lock().unwrap(), ["save"]);
    let notice = view
        .read_with(cx, |view, _| view.clipboard_notice())
        .unwrap();
    assert_eq!(notice.text, navo_l10n::tr("update-restart-stopped-save"));
}

#[gpui_kit::test]
fn restart_offers_hide_and_settings_stay_shut_while_a_restart_runs(cx: &mut TestAppContext) {
    let (window, view, _) = held_window(cx);
    add_job(&view, cx, 1);
    request(window, &view, cx);
    let has_entry = |view: &NavoWindow| {
        view.palette_entries()
            .iter()
            .any(|e| e.spec.name == "app.restart_update")
    };
    view.read_with(cx, |view, _| {
        assert!(view.restart_available());
        assert!(!view.restart_offered());
        assert!(!has_entry(view));
    });
    view.update(cx, |view, _| {
        view.settings.updates.restart = Restart::Applying
    });
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| view.toggle_settings(window, cx));
    })
    .unwrap();
    assert!(!view.read_with(cx, |view, _| view.settings.is_open()));
    view.update(cx, |view, _| view.settings.updates.restart = Restart::Idle);
    view.read_with(cx, |view, _| assert!(has_entry(view)));
}

#[gpui_kit::test]
fn recently_deleted_work_holds_the_restart_until_it_ends(cx: &mut TestAppContext) {
    let (window, view, calls) = held_window(cx);
    view.update(cx, |view, _| view.recent.start_work_for_test(7));
    request(window, &view, cx);
    assert_eq!(restart_state(&view, cx), Restart::Asking);
    assert_eq!(view.read_with(cx, |view, _| view.restart_job_count()), 1);
    assert!(calls.lock().unwrap().is_empty());
    view.update(cx, |view, cx| {
        view.recent = super::super::recent_sheet::RecentPanel::new(cx);
        cx.notify();
    });
    frame(window, cx);
    assert_eq!(*calls.lock().unwrap(), ["save", "apply"]);
}

#[gpui_kit::test]
fn an_unsaved_viewer_edit_stops_the_restart_with_a_notice(cx: &mut TestAppContext) {
    let (window, view, _) = viewer_window(cx, Ok("hello\n"), false);
    view.update(cx, |view, _| {
        view.identity.install = crate::app_menu::Install::Installed;
        view.settings.updates.view = ready();
    });
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    press(cx, window, &view, "ctrl-e");
    type_in(cx, window, &view, "x");
    request(window, &view, cx);
    assert_eq!(restart_state(&view, cx), Restart::Idle);
    let notice = view
        .read_with(cx, |view, _| view.clipboard_notice())
        .unwrap();
    assert_eq!(notice.text, navo_l10n::tr("update-restart-stopped-unsaved"));
    assert!(notice.is_error);
    assert!(view.read_with(cx, |view, _| view.viewer.is_some()));
}

#[gpui_kit::test]
fn a_command_center_action_does_nothing_while_the_restart_applies(cx: &mut TestAppContext) {
    let (window, view, _) = held_window(cx);
    let run = |view: &Entity<NavoWindow>, cx: &mut TestAppContext| {
        cx.update_window(window, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.run_action("test::Unregistered", window, cx)
            });
        })
        .unwrap();
        view.read_with(cx, |view, _| view.clipboard_notice())
            .map(|notice| notice.text.to_string())
    };
    let missing = Some(navo_l10n::tr("command-center-action-missing"));
    assert_eq!(run(&view, cx), missing);
    view.update(cx, |view, _| {
        view.show_notice(crate::status_bar::StatusNotice {
            text: "sentinel".into(),
            is_error: false,
        });
        view.settings.updates.restart = Restart::Applying;
    });
    assert_eq!(run(&view, cx).as_deref(), Some("sentinel"));
}
