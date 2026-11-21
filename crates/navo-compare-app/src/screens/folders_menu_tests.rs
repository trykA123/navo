//! Tests of the Folders context menu: right click, copy paths, expand all and the shortcut text.

use std::path::PathBuf;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent,
    TestAppContext, VisualTestContext,
};

use super::menu::cache_toast;
use super::tests::{shown, view_with};
use super::{FoldersView, bind_keys};
use crate::layers::toast::{self, ToastHost};
use crate::menu_host::{bind_menu_keys, menu_view};
use crate::screens::folders_head::SideRoot;
use sidewise_host::compare::hash_cache::file::ClearError;
use sidewise_l10n::t;

/// A focused view over the fixture with roots `/left` and `/right`, menu keys bound.
fn setup(cx: &mut TestAppContext) -> (AnyWindowHandle, gpui_kit::Entity<FoldersView>) {
    let roots = [abs_root("left"), abs_root("right")].map(SideRoot::folder);
    let (window, view, _) = view_with(cx, move |cx| FoldersView::new(cx).with_roots(roots));
    cx.update(|cx| {
        bind_keys(cx);
        bind_menu_keys(cx);
    });
    (window, view)
}

/// Presses and releases the right button on the centre of line `index`.
fn right_click(cx: &mut TestAppContext, window: AnyWindowHandle, index: usize) {
    let at = cx
        .update_window(window, |_, w, _| {
            w.find(("folder-line", index)).bounds().center()
        })
        .expect("window");
    let mut vcx = VisualTestContext::from_window(window, cx);
    vcx.simulate_event(MouseDownEvent {
        button: MouseButton::Right,
        position: at,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    vcx.simulate_event(MouseUpEvent {
        button: MouseButton::Right,
        position: at,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
}

/// The command lines of the open menu: (text, accelerator, enabled); separators are skipped.
fn menu_lines(
    cx: &mut TestAppContext,
    view: &gpui_kit::Entity<FoldersView>,
) -> Vec<(String, Option<String>, bool)> {
    let menu = view.read_with(cx, |view, _| menu_view(&view.menu));
    let menu = menu.expect("open menu");
    menu.read_with(cx, |menu, _| {
        let sep = sidewise_core::shell_menu::ItemKind::Separator;
        let items = menu.menu().items.iter().filter(|item| item.kind != sep);
        items
            .map(|i| (i.text.clone(), i.accelerator.clone(), i.enabled))
            .collect()
    })
}

/// Expands `a/b`, then right-clicks `a/b/y.txt` (only on the left) from another cursor line.
fn right_click_the_left_only_file(
    cx: &mut TestAppContext,
) -> (AnyWindowHandle, gpui_kit::Entity<FoldersView>) {
    let (window, view) = setup(cx);
    cx.update(|cx| {
        view.update(cx, |this, cx| {
            this.select_path("a/b/y.txt", cx);
            this.select_path("a", cx);
        })
    });
    cx.update_window(window, |_, window, _| window.refresh())
        .expect("window");
    cx.run_until_parked();
    assert_eq!(shown(&view, cx)[3], "a/b/y.txt");
    right_click(cx, window, 3);
    cx.run_until_parked();
    (window, view)
}

#[gpui_kit::test]
fn right_click_selects_and_shows_the_file_menu(cx: &mut TestAppContext) {
    let (_, view) = right_click_the_left_only_file(cx);
    cx.update(|cx| assert_eq!(view.read(cx).selected_path().as_deref(), Some("a/b/y.txt")));
    let lines = menu_lines(cx, &view);
    let labels: Vec<_> = lines.iter().map(|l| l.0.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Open diff",
            "Open in Stream",
            "Copy left path",
            "Copy right path",
            "Copy name",
            "Reveal left in Explorer",
            "Reveal right in Explorer",
            "Recheck all",
            "Clear compare cache"
        ]
    );
    let enabled: Vec<_> = lines.iter().map(|l| l.2).collect();
    let left = cfg!(windows);
    assert_eq!(
        enabled,
        [true, true, true, false, true, left, false, true, true]
    );
}

#[gpui_kit::test]
fn the_menu_shows_shift_enter_on_open_in_stream(cx: &mut TestAppContext) {
    let (_, view) = right_click_the_left_only_file(cx);
    let lines = menu_lines(cx, &view);
    assert_eq!(lines[0].1.as_deref(), Some("Enter"));
    assert_eq!(lines[1].1.as_deref(), Some("Shift+Enter"));
    assert_eq!(lines[2].1.as_deref(), Some("Ctrl+Shift+C"));
}

#[gpui_kit::test]
fn copy_left_path_writes_the_absolute_path(cx: &mut TestAppContext) {
    let (window, view) = setup(cx);
    cx.update(|cx| view.update(cx, |this, cx| this.select_path("a/x.txt", cx)));
    cx.simulate_keystrokes(window, "ctrl-shift-c");
    let text = cx.read_from_clipboard().and_then(|item| item.text());
    let want = abs_root("left").join("a").join("x.txt");
    assert_eq!(text.as_deref(), want.to_str());
}

/// A Git side (`app@v1`) on the left: Copy left path and Reveal left toast and copy nothing.
#[gpui_kit::test]
fn a_git_side_toasts_instead_of_copying_or_revealing(cx: &mut TestAppContext) {
    let roots = [
        SideRoot {
            path: abs_root("app"),
            rev: Some("v1".into()),
        },
        SideRoot::folder(abs_root("right")),
    ];
    let (window, view, _) = view_with(cx, move |cx| FoldersView::new(cx).with_roots(roots));
    cx.update(|cx| {
        bind_keys(cx);
        bind_menu_keys(cx);
    });
    let host = cx.new(|_| ToastHost::new());
    cx.update(|cx| {
        toast::install_sink(&host, cx);
        view.update(cx, |this, cx| this.select_path("a/x.txt", cx));
    });
    cx.simulate_keystrokes(window, "ctrl-shift-c");
    cx.simulate_keystrokes(window, "ctrl-shift-e");
    cx.run_until_parked();
    let want = t!("git-side-no-file", rev = "v1");
    let got = host.read_with(cx, |host, _| host.messages());
    assert_eq!(got, [want.clone(), want]);
    assert!(cx.read_from_clipboard().is_none());
}

#[gpui_kit::test]
fn shift_right_expands_all_below(cx: &mut TestAppContext) {
    let (window, view) = setup(cx);
    cx.simulate_keystrokes(window, "shift-right");
    assert_eq!(
        shown(&view, cx),
        ["a", "a/x.txt", "a/b", "a/b/y.txt", "c", "z.txt"]
    );
}

#[gpui_kit::test]
fn shift_f10_opens_the_menu_on_the_cursor_line(cx: &mut TestAppContext) {
    let (window, view) = setup(cx);
    cx.simulate_keystrokes(window, "shift-f10");
    cx.run_until_parked();
    let lines = menu_lines(cx, &view);
    assert_eq!(lines[0].0, "Expand");
    assert_eq!(lines[1].0, "Expand all");
}

/// An absolute test root: `/name` on Unix, `C:\name` on Windows, where a path without a drive
/// is not absolute and the app shows it drive-qualified.
fn abs_root(name: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(format!("C:\\{name}"))
    } else {
        PathBuf::from(format!("/{name}"))
    }
}

/// Whether the Folders view holds the focus.
fn view_focused(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &gpui_kit::Entity<FoldersView>,
) -> bool {
    cx.update_window(window, |_, w, cx| view.read(cx).focus.is_focused(w))
        .expect("window")
}

/// A mouse-chosen Copy left path gives the focus back to the tree and keeps the cursor line.
#[gpui_kit::test]
fn copy_by_mouse_refocuses_the_tree(cx: &mut TestAppContext) {
    let (window, view) = right_click_the_left_only_file(cx);
    cx.update_window(window, |_, w, cx| {
        w.render_frame(cx);
        w.within("context-menu")
            .within(("menu-level", 0usize))
            .click(("menu-row", 3usize), cx);
    })
    .expect("window");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.menu.is_none()));
    let text = cx.read_from_clipboard().and_then(|item| item.text());
    let want = abs_root("left").join("a").join("b").join("y.txt");
    assert_eq!(text.as_deref(), want.to_str());
    assert!(
        view_focused(cx, window, &view),
        "the tree had the focus when the menu opened"
    );
    cx.update(|cx| assert_eq!(view.read(cx).selected_path().as_deref(), Some("a/b/y.txt")));
}

/// Escape closes the Folders menu and gives the focus back to the tree.
#[gpui_kit::test]
fn escape_refocuses_the_tree(cx: &mut TestAppContext) {
    let (window, view) = right_click_the_left_only_file(cx);
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.menu.is_none()));
    assert!(view_focused(cx, window, &view));
}

/// The last two lines of every Folders menu are Recheck all and Clear compare cache, enabled.
#[gpui_kit::test]
fn the_menu_offers_recheck_and_clear(cx: &mut TestAppContext) {
    let (_, view) = right_click_the_left_only_file(cx);
    let lines = menu_lines(cx, &view);
    let tail: Vec<_> = lines[lines.len() - 2..]
        .iter()
        .map(|l| (l.0.as_str(), l.2))
        .collect();
    assert_eq!(tail, [("Recheck all", true), ("Clear compare cache", true)]);
}

/// Each outcome of a clear has its own toast text.
#[test]
fn cache_toast_names_each_outcome() {
    assert_eq!(cache_toast(&Ok(0)), "The compare cache is already empty.");
    assert_eq!(
        cache_toast(&Ok(1234)),
        "Cleared 1,234 files from the compare cache."
    );
    assert!(cache_toast(&Err(ClearError::Busy)).starts_with("Another Sidewise window"));
    let failed = cache_toast(&Err(ClearError::Io("disk full".into())));
    assert!(
        failed.starts_with("Could not clear the compare cache:") && failed.contains("disk full")
    );
}

/// Choosing Clear compare cache by mouse ends in a toast that names the compare cache.
#[gpui_kit::test]
fn clear_compare_cache_toasts(cx: &mut TestAppContext) {
    let (window, _) = right_click_the_left_only_file(cx);
    let host = cx.new(|_| ToastHost::new());
    cx.update(|cx| toast::install_sink(&host, cx));
    cx.update_window(window, |_, w, cx| {
        w.render_frame(cx);
        w.within("context-menu")
            .within(("menu-level", 0usize))
            .click(("menu-row", 10usize), cx);
    })
    .expect("window");
    cx.run_until_parked();
    let got = host.read_with(cx, |host, _| host.messages());
    assert!(got.iter().any(|m| m.contains("compare cache")), "{got:?}");
}
