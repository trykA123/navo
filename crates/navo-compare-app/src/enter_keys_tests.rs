//! Check of the owner's report "Enter does not start a compare": drives the real Enter key
//! through the shell, on the start fields, with scripted compare hooks (no real walk).

use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use crate::compare_tab_script::scripted_shell;
use crate::compare_tab_tests::TempDir;
use crate::shell::Shell;
use crate::start::{Side, SideHint};

type Opened = (AnyWindowHandle, Entity<Shell>);

/// Types `left` and `right` into the active start screen's fields, settles, focuses the left one.
fn typed(cx: &mut TestAppContext, (window, view): &Opened, left: &str, right: &str) {
    cx.update_window(*window, |_, window, cx| {
        let screen = view.read(cx).active_screen().expect("start").clone();
        screen.update(cx, |s, cx| {
            s.set_sides(left, right, window, cx);
            s.focus_side(Side::Left, window, cx);
        });
    })
    .expect("window");
    cx.run_until_parked();
}

/// Whether the shell has opened a compare tab.
fn compare_open(cx: &mut TestAppContext, (_, view): &Opened) -> bool {
    view.read_with(cx, |shell, _| shell.active_compare().is_some())
}

/// The two hints of the active start screen.
fn hints(cx: &mut TestAppContext, (_, view): &Opened) -> [SideHint; 2] {
    view.read_with(cx, |shell, cx| {
        let screen = shell.active_screen().expect("start").read(cx);
        [screen.hints[0].clone(), screen.hints[1].clone()]
    })
}

#[gpui_kit::test]
fn enter_in_a_field_with_plain_paths_starts_the_compare(cx: &mut TestAppContext) {
    let (l, r) = (TempDir::new("enter-ok-l"), TempDir::new("enter-ok-r"));
    let opened = scripted_shell(cx);
    typed(
        cx,
        &opened,
        &l.path().to_string_lossy(),
        &r.path().to_string_lossy(),
    );
    cx.simulate_keystrokes(opened.0, "enter");
    cx.run_until_parked();
    assert!(compare_open(cx, &opened));
}

/// A Windows "Copy as path" paste is wrapped in double quotes; Enter must still compare.
#[gpui_kit::test]
fn enter_with_pasted_quoted_paths_starts_the_compare(cx: &mut TestAppContext) {
    let (l, r) = (TempDir::new("enter-q-l"), TempDir::new("enter-q-r"));
    let opened = scripted_shell(cx);
    let quote = |t: &TempDir| format!("\"{}\"", t.path().to_string_lossy());
    typed(cx, &opened, &quote(&l), &quote(&r));
    cx.simulate_keystrokes(opened.0, "enter");
    cx.run_until_parked();
    assert!(
        compare_open(cx, &opened),
        "Enter did nothing; hints: {:?}",
        hints(cx, &opened)
    );
}
