use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, ElementId, TestAppContext};

use super::app_menu::{Rig, rig_search};
use crate::settings::{Field, Values};
use crate::window_view::{SearchHooks, ServiceHooks, ServiceStatus};

#[derive(Default)]
struct Calls {
    clear: AtomicUsize,
    install: AtomicUsize,
    remove: AtomicUsize,
    status_calls: AtomicUsize,
    status: Mutex<ServiceStatus>,
}

fn hooks(calls: &Arc<Calls>) -> SearchHooks {
    let (clear, status, install, remove) =
        (calls.clone(), calls.clone(), calls.clone(), calls.clone());
    SearchHooks {
        clear: Arc::new(move || {
            clear.clear.fetch_add(1, Ordering::SeqCst);
        }),
        service: ServiceHooks {
            status: Arc::new(move || {
                status.status_calls.fetch_add(1, Ordering::SeqCst);
                *status.status.lock().unwrap()
            }),
            install: Arc::new(move || {
                install.install.fetch_add(1, Ordering::SeqCst);
                *install.status.lock().unwrap() = ServiceStatus::Running;
            }),
            remove: Arc::new(move || {
                remove.remove.fetch_add(1, Ordering::SeqCst);
                *remove.status.lock().unwrap() = ServiceStatus::NotInstalled;
            }),
        },
        ..SearchHooks::none()
    }
}

fn open_search(
    cx: &mut TestAppContext,
    status: ServiceStatus,
    is_windows: bool,
) -> (Rig, Arc<Calls>) {
    let calls = Arc::new(Calls::default());
    *calls.status.lock().unwrap() = status;
    let rig = rig_search(cx, Values::default(), None, hooks(&calls));
    let view = rig.view.clone();
    cx.update_window(rig.window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.set_settings_windows(is_windows);
            view.open_settings_at(Field::SearchIndex, window, cx);
        });
    })
    .expect("window");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    view.read_with(cx, |view, _| view.scroll_settings_to_end());
    cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    (rig, calls)
}

fn label(cx: &mut TestAppContext, rig: &Rig, id: impl Into<ElementId>) -> Option<String> {
    let id = id.into();
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .filter(|found| found.visible())
            .and_then(|found| found.label().map(str::to_owned))
    })
    .expect("window")
}

fn count(value: &AtomicUsize) -> usize {
    value.load(Ordering::SeqCst)
}

#[gpui_kit::test]
fn clear_search_history_calls_the_hook_and_says_so(cx: &mut TestAppContext) {
    let (rig, calls) = open_search(cx, ServiceStatus::NotAvailable, true);
    assert!(label(cx, &rig, "settings-history-cleared").is_none());
    rig.click(cx, "settings-clear-history");
    assert_eq!(count(&calls.clear), 1);
    assert!(label(cx, &rig, "settings-history-cleared").is_some());
}

#[gpui_kit::test]
fn a_missing_service_shows_its_status_and_both_buttons_do_nothing(cx: &mut TestAppContext) {
    let (rig, calls) = open_search(cx, ServiceStatus::NotAvailable, true);
    assert_eq!(
        label(cx, &rig, "settings-service-status"),
        Some(ServiceStatus::NotAvailable.text().to_string())
    );
    rig.click(cx, "settings-service-install");
    rig.click(cx, "settings-service-remove");
    assert_eq!((count(&calls.install), count(&calls.remove)), (0, 0));
}

#[gpui_kit::test]
fn install_works_only_when_not_installed(cx: &mut TestAppContext) {
    let (rig, calls) = open_search(cx, ServiceStatus::NotInstalled, true);
    assert_eq!(
        label(cx, &rig, "settings-service-status"),
        Some(ServiceStatus::NotInstalled.text().to_string())
    );
    rig.click(cx, "settings-service-remove");
    assert_eq!(count(&calls.remove), 0);
    rig.click(cx, "settings-service-install");
    assert_eq!(count(&calls.install), 1);
}

#[gpui_kit::test]
fn remove_works_while_the_service_runs_or_is_stopped(cx: &mut TestAppContext) {
    for status in [ServiceStatus::Running, ServiceStatus::Stopped] {
        let (rig, calls) = open_search(cx, status, true);
        assert_eq!(
            label(cx, &rig, "settings-service-status"),
            Some(status.text().to_string())
        );
        rig.click(cx, "settings-service-install");
        assert_eq!(count(&calls.install), 0);
        rig.click(cx, "settings-service-remove");
        assert_eq!(count(&calls.remove), 1);
    }
}

#[gpui_kit::test]
fn without_windows_the_page_says_the_service_is_a_windows_feature(cx: &mut TestAppContext) {
    let (rig, _) = open_search(cx, ServiceStatus::NotAvailable, false);
    assert!(label(cx, &rig, "settings-service-note").is_some());
    assert!(label(cx, &rig, "settings-service-status").is_none());
}

#[gpui_kit::test]
fn service_actions_finish_after_the_click_handler_and_refresh_status(cx: &mut TestAppContext) {
    for (before, button, after) in [
        (
            ServiceStatus::NotInstalled,
            "settings-service-install",
            ServiceStatus::Running,
        ),
        (
            ServiceStatus::Running,
            "settings-service-remove",
            ServiceStatus::NotInstalled,
        ),
    ] {
        let (rig, calls) = open_search(cx, before, true);
        cx.update_window(rig.window, |_, window, cx| {
            window.click(button, cx);
            assert_eq!(count(&calls.install) + count(&calls.remove), 0);
            rig.view.update(cx, |view, cx| {
                view.toggle_settings(window, cx);
                view.open_settings_at(Field::SearchIndex, window, cx);
            });
        })
        .expect("window");
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        label(cx, &rig, "settings-service-status");
        rig.view
            .read_with(cx, |view, _| view.scroll_settings_to_end());
        assert_eq!(
            label(cx, &rig, "settings-service-status"),
            Some(after.text().to_string())
        );
        assert_eq!(count(&calls.status_calls), 2);
    }
}

#[gpui_kit::test]
fn rendering_uses_cached_status_and_each_search_open_refreshes_once(cx: &mut TestAppContext) {
    let (rig, calls) = open_search(cx, ServiceStatus::Running, true);
    assert_eq!(count(&calls.status_calls), 1);
    calls.status_calls.store(0, Ordering::SeqCst);
    for _ in 0..3 {
        label(cx, &rig, "settings-service-status");
    }
    assert_eq!(count(&calls.status_calls), 0);
    rig.press(cx, "escape");
    let view = rig.view.clone();
    cx.update_window(rig.window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.open_settings_at(Field::SearchIndex, window, cx)
        });
    })
    .expect("window");
    cx.run_until_parked();
    assert_eq!(count(&calls.status_calls), 1);
}
