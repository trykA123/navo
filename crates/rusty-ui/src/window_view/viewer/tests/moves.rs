use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use super::find::{line_start, numbered_lines};
use super::{Log, Setup, press_only, redraw, text_window, top};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{press, settle, type_in};

struct Open<'a> {
    window: AnyWindowHandle,
    view: &'a Entity<FerriteWindow>,
    log: &'a Log,
}

fn press_without_reading(cx: &mut TestAppContext, open: &Open<'_>, key: &str) {
    let Open { window, view, log } = *open;
    let (reads, before) = (log.reads(), top(cx, view));
    press_only(cx, window, key);
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    assert_eq!(log.reads(), reads, "{key} read the file on the UI thread");
    assert_eq!(top(cx, view), before, "{key} moved before its job ran");
    settle(cx, view);
    redraw(cx, window);
}

#[gpui_kit::test]
fn paged_moves_and_kind_switches_never_read_on_the_ui_thread(cx: &mut TestAppContext) {
    let bytes = numbered_lines();
    let len = bytes.len() as u64;
    let (window, view, log) = text_window(cx, Setup::huge(bytes));
    let open = Open {
        window,
        view: &view,
        log: &log,
    };
    for key in ["down", "pagedown", "up"] {
        press_without_reading(cx, &open, key);
    }
    assert!(top(cx, &view) > 0);
    press_without_reading(cx, &open, "ctrl-end");
    let end = top(cx, &view);
    assert!(end > len / 2);
    for key in ["alt-3", "alt-1", "ctrl-home"] {
        press_without_reading(cx, &open, key);
    }
    assert_eq!(top(cx, &view), 0);
}

#[gpui_kit::test]
fn go_to_line_is_resolved_in_the_background(cx: &mut TestAppContext) {
    let bytes = numbered_lines();
    let line_500 = line_start(&bytes, 500);
    let (window, view, log) = text_window(cx, Setup::huge(bytes));
    press(cx, window, &view, "ctrl-g");
    type_in(cx, window, &view, "500");
    let open = Open {
        window,
        view: &view,
        log: &log,
    };
    press_without_reading(cx, &open, "enter");
    assert_eq!(top(cx, &view), line_500);
}
