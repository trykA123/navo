//! Key tests of the shell: `Ctrl+\`, F6 / Shift+F6, hover-tip Escape, and the footer at 500 px Main.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::base::test_support::texts;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, Modifiers, Pixels, Point, TestAppContext,
    VisualTestContext, point, px,
};
use sidewise_kit::tokens::Mode;

use super::shell_at;
use crate::chrome::sidebar::DismissTip;
use crate::shell::Shell;
use crate::start::Side;

/// Whether the sidebar is collapsed to its rail.
fn rail(cx: &mut TestAppContext, view: &Entity<Shell>) -> bool {
    view.read_with(cx, |shell, cx| shell.sidebar().read(cx).rail())
}

/// Which start field holds the focus (`None` when focus is elsewhere, e.g. in the sidebar).
fn side(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<Shell>) -> Option<Side> {
    cx.update_window(window, |_, window, cx| {
        let screen = view.read(cx).active_screen().expect("start").clone();
        screen.read(cx).focused_side(window, cx)
    })
    .expect("window")
}

/// Whether the sidebar holds the focus.
fn in_sidebar(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<Shell>) -> bool {
    cx.update_window(window, |_, window, cx| {
        view.read(cx).sidebar().read(cx).contains_focus(window, cx)
    })
    .expect("window")
}

/// Moves the pointer to `pt` and renders until the hover tip's presence matches `want`.
fn hover_until_tip(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    pt: Point<Pixels>,
    want: bool,
) {
    VisualTestContext::from_window(window, cx).simulate_mouse_move(pt, None, Modifiers::none());
    for _ in 0..6 {
        cx.run_until_parked();
        let shown = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                window.try_find("rail-tooltip").is_some()
            })
            .expect("window");
        if shown == want {
            return;
        }
    }
    panic!("rail tip did not settle to {want}");
}

/// `Ctrl+\` collapses once from a typing field (the `\` is not typed) and expands once from the
/// sidebar, where the sidebar's own binding wins without a second toggle from the shell.
#[gpui_kit::test]
fn ctrl_backslash_toggles_the_rail_once_from_main_and_from_the_sidebar(cx: &mut TestAppContext) {
    let (window, view) = shell_at(cx, 780., Mode::Light);
    cx.simulate_input(window, "a\\b");
    assert_eq!(
        cx.update_window(window, |_, window, _| window
            .find("start-left")
            .value()
            .map(String::from))
            .expect("window"),
        Some("a\\b".into())
    );
    assert!(!rail(cx, &view));
    cx.simulate_keystrokes(window, "ctrl-\\");
    assert!(rail(cx, &view), "Main Ctrl+\\ collapses the sidebar");
    assert_eq!(
        side(cx, window, &view),
        Some(Side::Left),
        "Main keeps focus"
    );
    let typed = cx
        .update_window(window, |_, window, _| {
            window.find("start-left").value().map(String::from)
        })
        .expect("window");
    assert_eq!(typed, Some("a\\b".into()), "the shortcut is not typed");

    cx.update_window(window, |_, window, cx| {
        let sidebar = view.read(cx).sidebar().clone();
        sidebar.update(cx, |sidebar, cx| sidebar.focus_primary(window, cx));
    })
    .expect("window");
    cx.simulate_keystrokes(window, "ctrl-\\");
    assert!(!rail(cx, &view), "sidebar Ctrl+\\ expands exactly once");
}

/// F6 and Shift+F6 swap focus between the two panes; Tab stays inside the start screen.
#[gpui_kit::test]
fn f6_and_shift_f6_move_focus_between_the_real_panes(cx: &mut TestAppContext) {
    let (window, view) = shell_at(cx, 780., Mode::Light);
    assert_eq!(side(cx, window, &view), Some(Side::Left));
    cx.simulate_keystrokes(window, "tab");
    assert_eq!(
        side(cx, window, &view),
        Some(Side::Right),
        "Tab stays in Start"
    );
    assert!(!in_sidebar(cx, window, &view));

    cx.simulate_keystrokes(window, "f6");
    assert!(in_sidebar(cx, window, &view), "F6 goes to the sidebar");
    assert_eq!(side(cx, window, &view), None);
    cx.simulate_keystrokes(window, "f6");
    assert!(!in_sidebar(cx, window, &view), "F6 returns to Main");
    assert_eq!(side(cx, window, &view), Some(Side::Left));

    cx.simulate_keystrokes(window, "shift-f6");
    assert!(
        in_sidebar(cx, window, &view),
        "Shift+F6 goes to the sidebar"
    );
    cx.simulate_keystrokes(window, "shift-f6");
    assert_eq!(side(cx, window, &view), Some(Side::Left));
}

/// Escape from Main dismisses a shown hover tip without moving focus; a resting Escape propagates.
#[gpui_kit::test]
fn escape_from_main_dismisses_the_hover_tip_and_otherwise_propagates(cx: &mut TestAppContext) {
    let (window, view) = shell_at(cx, 780., Mode::Light);
    let reached = Rc::new(Cell::new(0_u32));
    let seen = reached.clone();
    cx.update(|cx| {
        cx.on_action(move |_: &DismissTip, _| seen.set(seen.get() + 1));
    });
    cx.simulate_keystrokes(window, "ctrl-\\");
    assert!(rail(cx, &view));
    assert_eq!(side(cx, window, &view), Some(Side::Left));

    let row = cx
        .update_window(window, |_, window, _| {
            window.find("new-compare").bounds().center()
        })
        .expect("window");
    hover_until_tip(cx, window, row, true);
    cx.simulate_keystrokes(window, "escape");
    hover_until_tip(cx, window, row, false);
    assert_eq!(reached.get(), 0, "a dismissing Escape is consumed");
    assert_eq!(
        side(cx, window, &view),
        Some(Side::Left),
        "Main keeps focus"
    );

    cx.simulate_keystrokes(window, "escape");
    assert_eq!(reached.get(), 1, "a resting Escape propagates");
    assert_eq!(side(cx, window, &view), Some(Side::Left));
    hover_until_tip(cx, window, point(px(700.), px(500.)), false);
}

/// At 780 px (500 px Main) the summary, all three labels and all three `Kbd` badges fit the
/// footer in order, without overlap, in Light and Dark.
#[gpui_kit::test]
fn footer_summary_labels_and_kbds_fit_at_500_px_main_light_and_dark(cx: &mut TestAppContext) {
    for mode in [Mode::Light, Mode::Dark] {
        let (window, _) = shell_at(cx, 780., mode);
        cx.update_window(window, |_, window, _| {
            let bar = window.find("status-bar").bounds();
            let (left, right) = (bar.origin.x, bar.origin.x + bar.size.width);
            assert_eq!(left, px(280.));
            assert_eq!(right, px(780.));
            let all = texts(window);
            let text = |wanted: &str| {
                all.iter()
                    .find(|t| t.visible() && t.text() == wanted)
                    .unwrap_or_else(|| panic!("{mode:?}: footer text {wanted:?} is missing"))
                    .bounds()
            };
            let mut edge = text("New compare · 1 open tab");
            assert!(edge.origin.x >= left && edge.size.width > px(0.));
            let mut previous_right = edge.origin.x + edge.size.width;
            edge = window.find("status-hints").bounds();
            assert!(
                edge.origin.x >= previous_right,
                "{mode:?}: summary overlaps hints"
            );
            assert!(
                edge.origin.x + edge.size.width <= right,
                "{mode:?}: hints escape"
            );
            previous_right = edge.origin.x;
            for (id, label) in [
                ("hint-new", "New tab"),
                ("hint-close", "Close tab"),
                ("hint-sidebar", "Sidebar"),
            ] {
                let (kbd, label) = (window.find(id).bounds(), text(label));
                assert!(kbd.size.width > px(0.), "{mode:?}: {id} badge collapsed");
                assert!(kbd.origin.x >= previous_right, "{mode:?}: {id} overlaps");
                assert!(
                    label.origin.x >= kbd.origin.x + kbd.size.width,
                    "{mode:?}: {id} label"
                );
                previous_right = label.origin.x + label.size.width;
                assert!(previous_right <= right - px(16.), "{mode:?}: {id} clipped");
            }
        })
        .expect("window");
    }
}
