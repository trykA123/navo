use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt;

use super::*;
use crate::settings::{DiagnosticRow, FIELDS};
use crate::window_view::tests::{fake, open_options, options_for, two_panes};

fn settings_window(
    cx: &TestAppContext,
    view: &gpui_kit::Entity<NavoWindow>,
) -> gpui_kit::AnyWindowHandle {
    view.read_with(cx, |view, _| view.settings.host.window_handle())
        .expect("settings window")
}

fn click_settings(
    cx: &mut TestAppContext,
    settings: gpui_kit::AnyWindowHandle,
    id: impl Into<gpui_kit::ElementId>,
) {
    let id = id.into();
    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn press_settings(cx: &mut TestAppContext, settings: gpui_kit::AnyWindowHandle, key: &str) {
    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        window.press(key, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn settings_diagnostics_lists_hook_rows_and_refreshes(cx: &mut TestAppContext) {
    let calls = Arc::new(AtomicUsize::new(0));
    let scans = calls.clone();
    let fs = fake();
    let mut options = options_for(fs.clone(), two_panes(&fs));
    options.settings.disk_use = Arc::new(move || {
        let count = scans.fetch_add(1, Ordering::SeqCst) + 1;
        vec![DiagnosticRow {
            name: format!("store {count}").into(),
            path: "redacted-store".into(),
            bytes: 2048,
            at_least: false,
            entries: Some(3),
            budget: Some(4096),
        }]
    });
    let (handle, view) = open_options(cx, options);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.open_settings(window, cx);
            view.select_setting(FIELDS.len(), window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    view.read_with(cx, |view, _| {
        assert_eq!(view.settings.diagnostics.len(), 1);
        assert_eq!(view.settings.diagnostics[0].name.as_ref(), "store 1");
    });
    let settings = settings_window(cx, &view);
    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .try_find(("settings-diagnostic-row", 0usize))
                .is_none()
        );
    })
    .unwrap();
    click_settings(cx, settings, "settings-storage-fold");
    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .try_find(("settings-diagnostic-row", 0usize))
                .is_some()
        );
    })
    .unwrap();
    click_settings(cx, settings, "settings-diagnostics-refresh");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    view.read_with(cx, |view, _| {
        assert_eq!(view.settings.diagnostics[0].name.as_ref(), "store 2");
    });
}

#[gpui_kit::test]
fn settings_diagnostics_empty_hook_renders_an_empty_state(cx: &mut TestAppContext) {
    let fs = fake();
    let (handle, view) = open_options(cx, options_for(fs.clone(), two_panes(&fs)));
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.open_settings(window, cx);
            view.select_setting(FIELDS.len(), window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    let settings = settings_window(cx, &view);
    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("settings-diagnostics-empty").is_some());
        assert!(
            window
                .try_find(("settings-diagnostic-row", 0usize))
                .is_none()
        );
    })
    .unwrap();
}

fn open_about_with_rows(
    cx: &mut TestAppContext,
    rows: Vec<DiagnosticRow>,
) -> (gpui_kit::AnyWindowHandle, gpui_kit::Entity<NavoWindow>) {
    let fs = fake();
    let mut options = options_for(fs.clone(), two_panes(&fs));
    options.settings.disk_use = Arc::new(move || rows.clone());
    let (handle, view) = open_options(cx, options);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.open_settings(window, cx);
            view.select_setting(FIELDS.len(), window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    (settings_window(cx, &view), view)
}

#[gpui_kit::test]
fn about_storage_hides_paths_until_fold_opens_and_inspects_selected_store(cx: &mut TestAppContext) {
    let store_path = "/tmp/navo-test-data/profile/cache.db";
    let (settings, view) = open_about_with_rows(
        cx,
        vec![DiagnosticRow {
            name: "Listing cache".into(),
            path: store_path.into(),
            bytes: 4096,
            at_least: false,
            entries: Some(12),
            budget: Some(8192),
        }],
    );
    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .try_find(("settings-diagnostic-row", 0usize))
                .is_none()
        );
        assert!(
            !gpui_kit::base::test_support::texts(window)
                .into_iter()
                .any(|item| item.text().contains(store_path)),
            "no store path may be drawn while collapsed"
        );
    })
    .unwrap();
    click_settings(cx, settings, "settings-storage-fold");
    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .try_find(("settings-diagnostic-row", 0usize))
                .is_some()
        );
    })
    .unwrap();
    view.read_with(cx, |view, _| view.scroll_settings_to_end());
    click_settings(cx, settings, ("settings-diagnostic-row", 0usize));
    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        let inspected = window
            .try_find("settings-diagnostic-path")
            .and_then(|found| found.label().map(str::to_owned));
        assert_eq!(inspected.as_deref(), Some(store_path));
    })
    .unwrap();
    click_settings(cx, settings, "settings-diagnostic-copy");
    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some(store_path)
        );
        assert!(window.try_find("settings-error").is_none());
        assert_eq!(
            window
                .try_find("settings-notice")
                .and_then(|found| found.label().map(str::to_owned))
                .as_deref(),
            Some("Path copied to clipboard.")
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn about_storage_up_and_down_step_through_open_store_rows(cx: &mut TestAppContext) {
    let first_path = "/tmp/navo-test-data/profile/cache.db";
    let second_path = "/tmp/navo-test-data/profile/navo.db";
    let (settings, view) = open_about_with_rows(
        cx,
        vec![
            DiagnosticRow {
                name: "Listing cache".into(),
                path: first_path.into(),
                bytes: 4096,
                at_least: false,
                entries: None,
                budget: None,
            },
            DiagnosticRow {
                name: "Journal".into(),
                path: second_path.into(),
                bytes: 2048,
                at_least: false,
                entries: None,
                budget: None,
            },
        ],
    );
    click_settings(cx, settings, "settings-storage-fold");
    view.read_with(cx, |view, _| view.scroll_settings_to_end());
    click_settings(cx, settings, ("settings-diagnostic-row", 0usize));
    press_settings(cx, settings, "down");
    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window
                .try_find("settings-diagnostic-path")
                .and_then(|found| found.label().map(str::to_owned))
                .as_deref(),
            Some(second_path)
        );
    })
    .unwrap();
    press_settings(cx, settings, "up");
    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window
                .try_find("settings-diagnostic-path")
                .and_then(|found| found.label().map(str::to_owned))
                .as_deref(),
            Some(first_path)
        );
    })
    .unwrap();
    press_settings(cx, settings, "up");
    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("settings-diagnostic-path").is_none());
    })
    .unwrap();
    view.read_with(cx, |view, _| {
        assert_eq!(view.settings_row(), crate::settings::HELP_ROW);
    });
}

#[gpui_kit::test]
fn about_storage_open_data_folder_opens_tab_and_closes_settings(cx: &mut TestAppContext) {
    let fs = fake();
    fs.add_folder("home/navo-data");
    let data = fs.location("home/navo-data");
    let mut options = options_for(fs.clone(), two_panes(&fs));
    options.settings.data_folder = Some(data.clone());
    options.settings.disk_use = Arc::new(move || {
        vec![DiagnosticRow {
            name: "Listing cache".into(),
            path: data
                .join(&navo_core::ProviderPath::from_text("cache.db"))
                .expect("join")
                .path()
                .to_text()
                .into(),
            bytes: 1024,
            at_least: false,
            entries: None,
            budget: None,
        }]
    });
    let (handle, view) = open_options(cx, options);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.open_settings(window, cx);
            view.select_setting(FIELDS.len(), window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    let settings = settings_window(cx, &view);
    click_settings(cx, settings, "settings-storage-open-folder");
    view.read_with(cx, |view, _| {
        assert!(!view.settings.is_open());
        assert_eq!(
            view.browser
                .state()
                .active_tab()
                .location()
                .path()
                .to_text(),
            "home/navo-data"
        );
    });
}

#[gpui_kit::test]
fn about_storage_open_store_folder_opens_parent_and_closes_settings(cx: &mut TestAppContext) {
    let fs = fake();
    fs.add_folder("home/navo-data");
    let data = fs.location("home/navo-data");
    let store_path: crate::prelude::SharedString = data
        .join(&navo_core::ProviderPath::from_text("cache.db"))
        .expect("join")
        .path()
        .to_text()
        .into();
    let mut options = options_for(fs.clone(), two_panes(&fs));
    options.settings.disk_use = Arc::new(move || {
        vec![DiagnosticRow {
            name: "Listing cache".into(),
            path: store_path.clone(),
            bytes: 1024,
            at_least: false,
            entries: None,
            budget: None,
        }]
    });
    let (handle, view) = open_options(cx, options);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.open_settings(window, cx);
            view.select_setting(FIELDS.len(), window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    let settings = settings_window(cx, &view);
    click_settings(cx, settings, "settings-storage-fold");
    view.read_with(cx, |view, _| view.scroll_settings_to_end());
    click_settings(cx, settings, ("settings-diagnostic-row", 0usize));
    click_settings(cx, settings, "settings-diagnostic-open");
    view.read_with(cx, |view, _| {
        assert!(!view.settings.is_open());
        assert_eq!(
            view.browser
                .state()
                .active_tab()
                .location()
                .path()
                .to_text(),
            "home/navo-data"
        );
    });
}
