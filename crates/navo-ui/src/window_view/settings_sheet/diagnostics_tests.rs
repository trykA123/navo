use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt;

use super::*;
use crate::settings::DiagnosticRow;
use crate::window_view::tests::{fake, open_options, options_for, two_panes};

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
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .try_find(("settings-diagnostic-row", 0usize))
                .is_some()
        );
        window.click("settings-diagnostics-refresh", cx);
    })
    .unwrap();
    cx.run_until_parked();
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
    cx.update_window(handle, |_, window, cx| {
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
