//! U22' UI matrix: Violet in Light and Dark with one flip each way; the five alpha screens render
//! inside the window at 780 x 560 (and once at 1200 x 760), theme projections stay coherent, and
//! the shell keys still work on a compare tab.

use std::fs;

use gpui_kit::base::test_support::find;
use gpui_kit::component::theme::{Theme as KitTheme, ThemeMode};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, ElementId, Entity, Pixels, ReadGlobal, TestAppContext, px,
    size,
};
use navo_compare_profile::appearance::PaletteId;
use sidewise_kit::tokens::{self, Mode};
use sidewise_ui::window_view::ScreenEvent;

use crate::compare_tab_tests::{TempDir, shell, submit, wait_for_row};
use crate::screens::compare_tab::{CompareTab, TabView};
use crate::shell::Shell;
use crate::theme::{SidewiseTheme, apply_sidewise};
use crate::theme_data::palettes::{ModeTokens, PALETTES};

type Opened = (AnyWindowHandle, Entity<Shell>);

/// The Violet colour tokens of `mode`.
fn violet(mode: Mode) -> ModeTokens {
    let spec = PALETTES
        .iter()
        .find(|(id, _)| *id == PaletteId::Violet)
        .map(|(_, spec)| *spec)
        .expect("violet spec");
    if mode.is_dark() {
        spec.dark
    } else {
        spec.light
    }
}

/// The mode that is not `mode`.
fn other(mode: Mode) -> Mode {
    if mode.is_dark() {
        Mode::Light
    } else {
        Mode::Dark
    }
}

/// Applies Violet in `mode` and lets the window redraw.
fn set_mode(cx: &mut TestAppContext, (window, _): &Opened, mode: Mode) {
    cx.update(|cx| apply_sidewise(PaletteId::Violet, mode, cx));
    cx.update_window(*window, |_, window, cx| window.render_frame(cx))
        .expect("window");
}

/// Asserts the three theme projections carry Violet in `mode` and none of the other mode's colours.
fn assert_theme(cx: &mut TestAppContext, mode: Mode, label: &str) {
    let (m, old) = (violet(mode), violet(other(mode)));
    let kit_mode = if mode.is_dark() {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    cx.update(|cx| {
        let side = SidewiseTheme::global(cx);
        assert_eq!(
            (side.palette, side.mode),
            (PaletteId::Violet, mode),
            "{label}: sidewise"
        );
        assert_eq!(side.spec.ring_px, 2.0, "{label}: focus ring is 2 px");
        let navo = tokens::current(cx);
        assert_eq!(navo.mode, mode, "{label}: navo mode");
        assert_eq!(
            navo.colors.background, m.surface,
            "{label}: navo background"
        );
        assert_eq!(navo.colors.text, m.text, "{label}: navo text");
        assert_eq!(navo.colors.accent, m.accent, "{label}: navo accent");
        assert_ne!(
            navo.colors.background, old.surface,
            "{label}: stale navo background"
        );
        assert_kit(KitTheme::global(cx), (&m, &old), (kit_mode, label));
    });
}

/// Asserts the kit theme carries the colours of `m` and none of `old` (the other mode's).
fn assert_kit(
    kit: &KitTheme,
    (m, old): (&ModeTokens, &ModeTokens),
    (mode, label): (ThemeMode, &str),
) {
    {
        assert_eq!(kit.mode, mode, "{label}: kit mode");
        assert_eq!(kit.colors.background, m.surface, "{label}: kit background");
        assert_eq!(kit.colors.foreground, m.text, "{label}: kit foreground");
        assert_eq!(kit.colors.primary, m.accent, "{label}: kit accent");
        assert_eq!(
            kit.tokens.background.color, m.surface,
            "{label}: kit token background"
        );
        assert_eq!(
            kit.tokens.foreground.color, m.text,
            "{label}: kit token foreground"
        );
        assert_eq!(
            kit.colors.scrollbar_thumb, m.sep_strong,
            "{label}: kit scrollbar thumb"
        );
        assert_eq!(
            kit.tokens.scrollbar_thumb.color, m.sep_strong,
            "{label}: kit token thumb"
        );
        assert_ne!(
            kit.colors.background, old.surface,
            "{label}: stale kit background"
        );
        assert_ne!(
            kit.colors.scrollbar_thumb, old.sep_strong,
            "{label}: stale kit thumb"
        );
    }
}

/// Renders a frame and asserts the main pane sits inside a window `width` wide.
fn assert_fits(cx: &mut TestAppContext, (window, _): &Opened, width: f32, label: &str) {
    cx.run_until_parked();
    cx.update_window(*window, |_, window, cx| {
        window.render_frame(cx);
        let snap = find(window, &[], &ElementId::Name("main-pane".into()))
            .unwrap_or_else(|| panic!("{label}: main pane missing"));
        let b = snap.bounds();
        let right: Pixels = b.origin.x + b.size.width;
        assert!(b.size.width > px(0.), "{label}: main pane has no width");
        assert!(
            right <= px(width),
            "{label}: main pane ends at {right:?} > {width}"
        );
    })
    .expect("window");
}

/// Types `keys`, lets the tab settle and returns the view it shows.
fn press(cx: &mut TestAppContext, open: &Opened, tab: &Entity<CompareTab>, keys: &str) -> TabView {
    cx.simulate_keystrokes(open.0, keys);
    cx.run_until_parked();
    tab.read_with(cx, |tab, _| tab.view())
}

/// Opens a shell in `mode` at `w` x `h`, resized before any compare starts.
fn open_at(cx: &mut TestAppContext, mode: Mode, (w, h): (f32, f32)) -> Opened {
    let opened = shell(cx);
    cx.simulate_window_resize(opened.0, size(px(w), px(h)));
    set_mode(cx, &opened, mode);
    opened
}

/// Submits a compare of two folders that differ in `a.txt`; returns the tab and the folders.
fn compare(cx: &mut TestAppContext, opened: &Opened) -> (Entity<CompareTab>, [TempDir; 2]) {
    let left = TempDir::new("u22-l");
    let right = TempDir::new("u22-r");
    fs::write(left.path().join("a.txt"), "one\n").expect("write left");
    fs::write(right.path().join("a.txt"), "two\n").expect("write right");
    submit(cx, opened, [left.path(), right.path()]);
    assert!(wait_for_row(cx, opened, "a.txt"), "rows reach the view");
    let tab = opened
        .1
        .read_with(cx, |shell, _| shell.active_compare().cloned());
    (tab.expect("compare tab"), [left, right])
}

/// Closes the File or Stream screen the way its own Escape does: by its `Closed` event.
fn close_screen(cx: &mut TestAppContext, open: &Opened, tab: &Entity<CompareTab>) {
    let screen = tab.read_with(cx, |tab, _| tab.screen().clone());
    cx.update_window(open.0, |_, _, cx| {
        screen.update(cx, |_, cx| cx.emit(ScreenEvent::Closed { reveal: None }));
    })
    .expect("window");
    cx.run_until_parked();
}

/// Visits Large, Folders, File and Stream by key and checks each fits `w` wide in `mode`.
fn walk(cx: &mut TestAppContext, o: &Opened, tab: &Entity<CompareTab>, (mode, w): (Mode, f32)) {
    let at = |view| format!("{mode:?} {view:?} at {w}");
    for (keys, want) in [("5", TabView::Large), ("2", TabView::Folders)] {
        assert_eq!(press(cx, o, tab, keys), want);
        assert_fits(cx, o, w, &at(want));
    }
    assert_eq!(press(cx, o, tab, "enter"), TabView::File);
    assert_fits(cx, o, w, &at(TabView::File));
    close_screen(cx, o, tab);
    assert_eq!(press(cx, o, tab, "4"), TabView::Stream);
    assert_fits(cx, o, w, &at(TabView::Stream));
    close_screen(cx, o, tab);
    assert_eq!(press(cx, o, tab, "2"), TabView::Folders);
}

/// Runs the whole matrix from `mode`: Start, the four views, a flip, the views again, a flip back.
fn matrix(cx: &mut TestAppContext, mode: Mode, dims: (f32, f32)) {
    let o = open_at(cx, mode, dims);
    assert_theme(cx, mode, &format!("{mode:?} start"));
    assert_fits(cx, &o, dims.0, &format!("{mode:?} Start at {}", dims.0));
    let (tab, _dirs) = compare(cx, &o);
    walk(cx, &o, &tab, (mode, dims.0));
    for flipped in [other(mode), mode] {
        set_mode(cx, &o, flipped);
        assert_theme(cx, flipped, &format!("{flipped:?} after flip"));
        walk(cx, &o, &tab, (flipped, dims.0));
    }
}

#[gpui_kit::test]
fn light_matrix_at_780_by_560(cx: &mut TestAppContext) {
    matrix(cx, Mode::Light, (780., 560.));
}

#[gpui_kit::test]
fn dark_matrix_at_780_by_560(cx: &mut TestAppContext) {
    matrix(cx, Mode::Dark, (780., 560.));
}

#[gpui_kit::test]
fn light_matrix_at_1200_by_760(cx: &mut TestAppContext) {
    matrix(cx, Mode::Light, (1200., 760.));
}

#[gpui_kit::test]
fn shell_keys_work_on_a_compare_tab(cx: &mut TestAppContext) {
    let o = open_at(cx, Mode::Light, (780., 560.));
    let (tab, _dirs) = compare(cx, &o);
    let count = |cx: &mut TestAppContext| o.1.read_with(cx, |s, _| (s.tab_count(), s.active()));
    cx.simulate_keystrokes(o.0, "ctrl-t");
    assert_eq!(count(cx), (2, 1), "ctrl-t adds a start tab");
    cx.simulate_keystrokes(o.0, "ctrl-shift-tab");
    assert_eq!(
        count(cx),
        (2, 0),
        "ctrl-shift-tab returns to the compare tab"
    );
    cx.simulate_keystrokes(o.0, "ctrl-tab");
    assert_eq!(count(cx), (2, 1), "ctrl-tab goes on");
    cx.simulate_keystrokes(o.0, "ctrl-w");
    assert_eq!(count(cx), (1, 0), "ctrl-w closes the start tab");
    cx.run_until_parked();
    assert_eq!(
        press(cx, &o, &tab, "5"),
        TabView::Large,
        "digits work after focus returns"
    );
    assert_eq!(
        press(cx, &o, &tab, "escape"),
        TabView::Folders,
        "escape leaves Large"
    );
    cx.simulate_keystrokes(o.0, "ctrl-\\");
    assert_fits(cx, &o, 780., "rail toggled");
}
