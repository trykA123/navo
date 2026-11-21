//! Tests of the topbar (NC66): every pill and action key fires the registered callback, and
//! the bar fits the 780 px minimum width without clipping or losing its side insets.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::rc::Rc;

use gpui_kit::base::test_support::{find, snapshots, texts};
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Context, ElementId, Entity, IntoElement, Modifiers,
    ParentElement, Point, Render, Role, SharedString, Styled, TestAppContext, VisualTestContext,
    Window, WindowBounds, WindowOptions, div, point, px, size,
};
use navo_compare_profile::appearance::PaletteId;
use sidewise_kit::tokens::Mode;

use super::topbar_status::{Action, Crumbs, MIN_W, Topbar, TopbarMode, bind_keys};
use crate::theme::apply_sidewise;
use crate::theme_data::palettes::DIMS;

/// The element ids of the search pill and the three action pills, in bar order.
const IDS: [(&str, Action); 5] = [
    ("top-search", Action::Search),
    ("top-theme", Action::Theme),
    ("top-view", Action::View),
    ("top-keys", Action::Keys),
    ("top-sync", Action::Sync),
];

/// Opens a focused, drawn topbar in a Light-themed window of `width`, recording every action fired.
pub(super) fn topbar(
    cx: &mut TestAppContext,
    width: f32,
) -> (AnyWindowHandle, Entity<Topbar>, Rc<RefCell<Vec<Action>>>) {
    topbar_in(cx, width, Mode::Light)
}

/// Opens a focused, drawn topbar in `mode` at `width`, recording every action fired.
fn topbar_in(
    cx: &mut TestAppContext,
    width: f32,
    mode: Mode,
) -> (AnyWindowHandle, Entity<Topbar>, Rc<RefCell<Vec<Action>>>) {
    let fired = Rc::new(RefCell::new(Vec::new()));
    cx.update(|cx| {
        gpui_kit::init(cx);
        apply_sidewise(PaletteId::Violet, mode, cx);
        bind_keys(cx);
    });
    let log = Rc::clone(&fired);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: Point::default(),
            size: size(px(width), px(600.)),
        })),
        ..Default::default()
    };
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| {
                Topbar::new(
                    Crumbs {
                        title: "New comparison".into(),
                        detail: "Folder · File".into(),
                    },
                    move |action, _| log.borrow_mut().push(action),
                    window,
                    cx,
                )
            })
        })
        .expect("open test window")
    });
    cx.update_window(window, |_, window, cx| {
        let handle = view.read(cx).focus_handle();
        handle.focus(window, cx);
        window.render_frame(cx);
    })
    .expect("window");
    (window, view, fired)
}

/// Clicks the centre of one observed element with a real mouse down and up.
fn click(cx: &mut TestAppContext, window: AnyWindowHandle, id: &str) {
    let at = cx
        .update_window(window, |_, window, _| {
            let found = find(window, &[], &ElementId::Name(id.into()))
                .unwrap_or_else(|| panic!("no {id} element in the drawn topbar"));
            let b = found.bounds();
            point(
                b.origin.x + b.size.width / 2.0,
                b.origin.y + b.size.height / 2.0,
            )
        })
        .expect("window");
    VisualTestContext::from_window(window, cx).simulate_click(at, Modifiers::none());
}

/// Clicking each pill fires its own registered action, once.
#[gpui_kit::test]
fn every_pill_fires_its_registered_action(cx: &mut TestAppContext) {
    let (window, _view, fired) = topbar(cx, 1024.);
    for (id, action) in IDS {
        click(cx, window, id);
        assert_eq!(
            fired.borrow().last(),
            Some(&action),
            "clicking {id} must fire {action:?}"
        );
    }
    assert_eq!(fired.borrow().len(), IDS.len(), "one callback per click");
}

/// Alt+V, Alt+S and / fire the same callbacks as their pills.
#[gpui_kit::test]
fn the_action_keys_fire_the_same_callbacks(cx: &mut TestAppContext) {
    let (window, _view, fired) = topbar(cx, 1024.);
    for (keys, action) in [
        ("alt-v", Action::View),
        ("alt-s", Action::Sync),
        ("/", Action::Search),
    ] {
        cx.simulate_keystrokes(window, keys);
        assert_eq!(
            fired.borrow().last(),
            Some(&action),
            "{keys} must fire {action:?}"
        );
    }
}

/// At the minimum width no visible part of the bar overflows the window either side.
#[gpui_kit::test]
fn the_bar_fits_the_minimum_width_without_clipping(cx: &mut TestAppContext) {
    let (window, _view, _fired) = topbar(cx, MIN_W);
    cx.update_window(window, |_, window, _| {
        let viewport = window.viewport_size();
        let mut checked = 0;
        for snap in snapshots(window) {
            let b = snap.bounds();
            if !snap.visible() || b.size.width <= px(0.) {
                continue;
            }
            assert!(
                b.origin.x >= px(0.),
                "{:?} starts left of the window at {MIN_W}px",
                snap.path()
            );
            assert!(
                b.origin.x + b.size.width <= viewport.width,
                "{:?} overflows the right edge at {MIN_W}px",
                snap.path()
            );
            checked += 1;
        }
        assert!(checked >= IDS.len(), "the pills must all be drawn");
    })
    .expect("window");
}

/// The bar keeps the owner's 16 px side inset for its text at the minimum width.
#[gpui_kit::test]
fn the_text_keeps_the_side_insets(cx: &mut TestAppContext) {
    let (window, _view, _fired) = topbar(cx, MIN_W);
    cx.update_window(window, |_, window, _| {
        let viewport = window.viewport_size();
        let first = find(window, &[], &ElementId::Name("top-search".into()))
            .expect("search pill")
            .bounds();
        let last = find(window, &[], &ElementId::Name("top-sync".into()))
            .expect("sync pill")
            .bounds();
        assert!(
            first.origin.x >= DIMS.inset_x,
            "the bar's content must start at least 16px in"
        );
        assert!(
            last.origin.x + last.size.width <= viewport.width - DIMS.inset_x,
            "the bar's content must end at least 16px before the edge"
        );
    })
    .expect("window");
}

/// The filter text round-trips through `set_filter`, `filter()` and the rendered text field.
#[gpui_kit::test]
fn the_filter_text_round_trips(cx: &mut TestAppContext) {
    let (window, view, _fired) = topbar(cx, 1024.);
    cx.update_window(window, |_, window, cx| {
        assert_eq!(
            view.read(cx).filter(),
            &SharedString::default(),
            "starts empty"
        );
        view.update(cx, |bar, cx| bar.set_filter("main.rs", window, cx));
        window.render_frame(cx);
        assert_eq!(view.read(cx).filter(), &SharedString::from("main.rs"));
        assert_eq!(window.find("top-search").value(), Some("main.rs"));
    })
    .expect("window");
}

/// Pressing Tab reaches Keys without mouse input, and Enter/Space activates it.
#[gpui_kit::test]
fn tab_reaches_keys_and_activates_without_mouse(cx: &mut TestAppContext) {
    let (window, _view, fired) = topbar(cx, 1024.);
    cx.update_window(window, |_, window, cx| {
        let initial = window.find("top-keys");
        assert_eq!(
            initial.role(),
            Some(Role::Button),
            "Keys control must expose Role::Button"
        );
        let keys = window
            .try_find_by_label("Keys")
            .expect("Keys control must expose accessible label \"Keys\"");
        assert_eq!(keys.focused(), Some(false), "Keys starts unfocused");

        let mut reached = false;
        for _ in 0..=IDS.len() {
            window.press("tab", cx);
            if window.find_by_label("Keys").focused() == Some(true) {
                reached = true;
                break;
            }
        }
        assert!(reached, "Tab must reach Keys without a mouse");

        window.press("enter", cx);
        window.press("space", cx);
    })
    .expect("window");

    assert_eq!(
        fired.borrow().as_slice(),
        [Action::Keys, Action::Keys],
        "Enter and Space on focused Keys must each fire Action::Keys once"
    );
}

/// Verifies `top-search` owns the labeled text input and receives Tab focus.
fn assert_search_tab_focus(window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) {
    window.press("tab", cx);
    let search_by_id = window.find("top-search");
    let search_by_label = window.find_by_label("Search files");
    assert!(
        search_by_label.path().starts_with(search_by_id.path()),
        "top-search must own the Search files input"
    );
    assert_eq!(search_by_label.role(), Some(Role::TextInput));
    assert_eq!(search_by_label.focused(), Some(true));
}

/// Search has text-input semantics, and the four action pills have button semantics, accessible
/// labels, Tab/Shift-Tab focus, and Enter/Space keyboard activation.
#[gpui_kit::test]
fn every_control_has_button_semantics_tab_focus_and_keyboard_activation(cx: &mut TestAppContext) {
    const BUTTONS: [(&str, &str, Action); 4] = [
        ("top-theme", "Theme", Action::Theme),
        ("top-view", "View", Action::View),
        ("top-keys", "Keys", Action::Keys),
        ("top-sync", "Sync", Action::Sync),
    ];

    let (window, _view, fired) = topbar(cx, 1024.);
    cx.update_window(window, |_, window, cx| {
        assert_search_tab_focus(window, cx);
        for (idx, (id, label, action)) in BUTTONS.iter().enumerate() {
            window.press("tab", cx);
            let by_label = window.find_by_label(label);
            assert_eq!(window.find(*id).path(), by_label.path(), "{id} mismatch");
            assert_eq!(by_label.role(), Some(Role::Button), "{id} role");
            assert_eq!(by_label.focused(), Some(true), "Tab #{} {id}", idx + 1);
            let key = if idx % 2 == 0 { "enter" } else { "space" };
            window.press(key, cx);
            assert_eq!(fired.borrow().last(), Some(action), "{key} on {id}");
        }
        window.press("shift-tab", cx);
        assert_eq!(window.find_by_label("Keys").focused(), Some(true));
        window.press("space", cx);
    })
    .expect("window");

    assert_eq!(fired.borrow().len(), BUTTONS.len() + 1);
    assert_eq!(fired.borrow().last(), Some(&Action::Keys));
}

/// Shortcuts still fire when a pill holds focus, and Escape does not fire stale actions or trap focus.
#[gpui_kit::test]
fn shortcuts_work_from_focused_button_and_escape_does_not_trap(cx: &mut TestAppContext) {
    let (window, _view, fired) = topbar(cx, 1024.);
    cx.update_window(window, |_, window, cx| {
        for _ in 0..3 {
            window.press("tab", cx);
        }
        assert_eq!(window.find_by_label("View").focused(), Some(true));

        window.press("escape", cx);
        assert!(
            fired.borrow().is_empty(),
            "Escape on a resting topbar button must not fire any action"
        );
        window.press("tab", cx);
        assert_eq!(
            window.find_by_label("Keys").focused(),
            Some(true),
            "focus must still advance after Escape"
        );
    })
    .expect("window");

    for (keys, action) in [
        ("alt-v", Action::View),
        ("alt-s", Action::Sync),
        ("/", Action::Search),
    ] {
        cx.simulate_keystrokes(window, keys);
        assert_eq!(
            fired.borrow().last(),
            Some(&action),
            "{keys} must still fire {action:?} when a pill is focused"
        );
    }
}

/// In Dark mode at the 780 px minimum width, Tab reaches Keys and Enter and Space both activate it.
#[gpui_kit::test]
fn dark_mode_minimum_width_reaches_keys_by_keyboard(cx: &mut TestAppContext) {
    let (window, _view, fired) = topbar_in(cx, MIN_W, Mode::Dark);
    cx.update_window(window, |_, window, cx| {
        for _ in 0..4 {
            window.press("tab", cx);
        }
        let keys = window.find_by_label("Keys");
        assert_eq!(keys.focused(), Some(true), "Tab #4 must focus Keys");
        assert!(keys.visible(), "Keys must stay visible at {MIN_W}px");
        window.press("enter", cx);
        window.press("space", cx);
    })
    .expect("window");
    assert_eq!(fired.borrow().as_slice(), [Action::Keys, Action::Keys]);
}

/// Asserts that the visible TextField value, `Topbar::filter`, and change callback all agree.
fn assert_filter_synced(
    window: &gpui_kit::Window,
    bar: &Topbar,
    changes: &[SharedString],
    expected: &str,
) {
    assert_eq!(window.find("top-search").value(), Some(expected));
    assert_eq!(bar.filter().as_str(), expected);
    assert_eq!(changes.last().map(SharedString::as_str), Some(expected));
}

/// Pressing `/` focuses the inline filter input, real typing and Ctrl+A/Backspace clearing update
/// both `Topbar::filter` and the registered change callback, and Tab still reaches Keys.
#[gpui_kit::test]
fn slash_focuses_filter_real_typing_and_clearing_sync_callback_and_tab_navigates(
    cx: &mut TestAppContext,
) {
    let (window, view, fired) = topbar(cx, 1024.);
    let changes = Rc::new(RefCell::new(Vec::<SharedString>::new()));
    let sink = Rc::clone(&changes);
    cx.update(|cx| {
        view.update(cx, |bar, _| {
            bar.set_on_filter_change(move |text, _| sink.borrow_mut().push(text));
        });
    });

    cx.simulate_keystrokes(window, "/");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(fired.borrow().as_slice(), [Action::Search]);
        assert_eq!(window.find("top-search").focused(), Some(true));
        window.input("foo.rs", cx);
        window.render_frame(cx);
    })
    .expect("window");
    cx.run_until_parked();

    cx.update_window(window, |_, window, cx| {
        assert_filter_synced(window, view.read(cx), &changes.borrow(), "foo.rs");
        window.press("ctrl-a", cx);
        window.press("backspace", cx);
        window.render_frame(cx);
    })
    .expect("window");
    cx.run_until_parked();

    cx.update_window(window, |_, window, cx| {
        assert_filter_synced(window, view.read(cx), &changes.borrow(), "");
        for _ in 0..3 {
            window.press("tab", cx);
        }
        assert_eq!(window.find_by_label("Keys").focused(), Some(true));
        window.press("enter", cx);
    })
    .expect("window");

    assert_eq!(fired.borrow().as_slice(), [Action::Search, Action::Keys]);
}

/// At the 780 px minimum width in both Light and Dark modes, typing a long path into the inline
/// filter stays clipped cleanly inside the bar while keeping action buttons visible and reachable.
#[gpui_kit::test]
fn editable_filter_fits_light_and_dark_minimum_width_without_clipping(cx: &mut TestAppContext) {
    for mode in [Mode::Light, Mode::Dark] {
        let (window, view, _fired) = topbar_in(cx, MIN_W, mode);
        cx.simulate_keystrokes(window, "/");
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| {
            window.input("crates/navo-compare-app/src/chrome/topbar_status.rs", cx);
            window.render_frame(cx);
        })
        .expect("window");
        cx.run_until_parked();

        cx.update_window(window, |_, window, cx| {
            assert_eq!(
                view.read(cx).filter(),
                &SharedString::from("crates/navo-compare-app/src/chrome/topbar_status.rs"),
                "{mode:?}: long filter must update Topbar::filter"
            );

            let viewport = window.viewport_size();
            let search = find(window, &[], &ElementId::Name("top-search".into()))
                .expect("top-search")
                .bounds();
            let sync = find(window, &[], &ElementId::Name("top-sync".into()))
                .expect("top-sync")
                .bounds();
            assert!(
                search.origin.x >= DIMS.inset_x && search.size.width > px(0.),
                "{mode:?}: editable filter must keep side inset and non-zero width"
            );
            assert!(
                sync.origin.x + sync.size.width <= viewport.width - DIMS.inset_x,
                "{mode:?}: right-most pill must stay within {MIN_W}px inset"
            );

            for _ in 0..3 {
                window.press("tab", cx);
            }
            assert_eq!(
                window.find_by_label("Keys").focused(),
                Some(true),
                "{mode:?}: Tab from filter must reach Keys at {MIN_W}px"
            );
        })
        .expect("window");
    }
}

/// Checks the mode default, all-six enabled defaults, crumb update, and visibility matrix.
#[gpui_kit::test]
fn mode_visibility_matrix_defaults_and_crumbs(cx: &mut TestAppContext) {
    assert_eq!(TopbarMode::default(), TopbarMode::Folders);
    let expected = [
        (TopbarMode::Start, [false, true, true, false]),
        (TopbarMode::Folders, [true, true, true, true]),
        (TopbarMode::File, [false, false, true, true]),
        (TopbarMode::Git, [false, false, true, false]),
        (TopbarMode::Stream, [false, false, true, true]),
        (TopbarMode::Large, [false, false, true, true]),
    ];
    let actions = [Action::Search, Action::View, Action::Keys, Action::Sync];
    for (mode, flags) in expected {
        for (action, want) in actions.into_iter().zip(flags) {
            assert_eq!(mode.shows(action), want, "{mode:?}::shows({action:?})");
        }
        assert!(mode.shows(Action::Theme), "{mode:?} shows Theme");
        assert!(!mode.shows(Action::Accounts), "{mode:?} hides Accounts");
    }

    let (window, view, _fired) = topbar(cx, 1024.);
    cx.update_window(window, |_, window, cx| {
        for action in actions {
            assert!(
                view.read(cx).is_available(action),
                "{action:?} must be available by default"
            );
        }
        view.update(cx, |bar, cx| {
            bar.set_crumbs(
                Crumbs {
                    title: "Left vs Right".into(),
                    detail: "12 files".into(),
                },
                cx,
            );
        });
        window.render_frame(cx);
    })
    .expect("window");
}

/// Returns the drawn width of element `id`.
fn width_of(window: &gpui_kit::Window, id: &str) -> gpui_kit::Pixels {
    find(window, &[], &ElementId::Name(id.into()))
        .unwrap_or_else(|| panic!("missing {id}"))
        .bounds()
        .size
        .width
}

/// Asserts which element ids are mounted vs absent.
fn assert_mounted(window: &gpui_kit::Window, present: &[&str], hidden: &[&str]) {
    for id in present {
        assert!(
            find(window, &[], &ElementId::Name((*id).into())).is_some(),
            "{id} mounted"
        );
    }
    for id in hidden {
        assert!(
            find(window, &[], &ElementId::Name((*id).into())).is_none(),
            "{id} hidden"
        );
    }
}

/// Start mode hides Search and Sync; disabled pills omit hints and ignore clicks/shortcuts.
#[gpui_kit::test]
fn start_mode_hides_search_sync_and_disabled_pills_drop_hints_and_ignore_actions(
    cx: &mut TestAppContext,
) {
    let (window, view, fired) = topbar(cx, 1024.);
    let (vw, tw) = cx
        .update_window(window, |_, window, cx| {
            view.update(cx, |bar, cx| bar.set_mode(TopbarMode::Start, cx));
            window.render_frame(cx);
            assert_mounted(
                window,
                &["top-theme", "top-view", "top-keys"],
                &["top-search", "top-sync", "top-accounts"],
            );
            let widths = (width_of(window, "top-view"), width_of(window, "top-keys"));
            view.update(cx, |bar, cx| {
                bar.set_available(Action::View, false, cx);
                bar.set_available(Action::Keys, false, cx);
            });
            window.render_frame(cx);
            assert!(width_of(window, "top-view") < widths.0, "View hint omitted");
            assert!(width_of(window, "top-keys") < widths.1, "Keys hint omitted");
            widths
        })
        .expect("window");
    let _ = (vw, tw);

    click(cx, window, "top-view");
    click(cx, window, "top-keys");
    for key in ["alt-v", "alt-s", "/"] {
        cx.simulate_keystrokes(window, key);
    }
    cx.run_until_parked();
    assert!(
        fired.borrow().is_empty(),
        "hidden/disabled actions must not fire"
    );
}

/// Git hides View/Sync/Search while File shows Sync (View is Start/Folders only) and honors Alt+S availability.
#[gpui_kit::test]
fn git_and_file_modes_gate_sync_and_view_availability(cx: &mut TestAppContext) {
    let (window, view, fired) = topbar(cx, 1024.);
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |bar, cx| bar.set_mode(TopbarMode::Git, cx));
        window.render_frame(cx);
        assert_mounted(
            window,
            &["top-theme", "top-keys"],
            &["top-search", "top-view", "top-sync", "top-accounts"],
        );
        view.update(cx, |bar, cx| {
            bar.set_mode(TopbarMode::File, cx);
            bar.set_available(Action::Sync, false, cx);
        });
        window.render_frame(cx);
        assert_mounted(window, &["top-sync"], &["top-search", "top-view"]);
    })
    .expect("window");
    cx.simulate_keystrokes(window, "alt-s");
    click(cx, window, "top-sync");
    cx.run_until_parked();
    assert!(
        fired.borrow().is_empty(),
        "disabled Sync must ignore Alt+S and clicks"
    );
}

/// Mounted disabled Search in Folders ignores `/`, wrapper click, inner Input click+typing,
/// keeping root focus and filter text until re-enabled.
#[gpui_kit::test]
fn folders_disabled_search_blocks_slash_wrapper_and_inner_input_then_reenable_works(
    cx: &mut TestAppContext,
) {
    let (window, view, fired) = topbar(cx, 1024.);
    let wrapper_pt = cx
        .update_window(window, |_, window, cx| {
            view.update(cx, |bar, cx| bar.set_available(Action::Search, false, cx));
            window.render_frame(cx);
            assert!(view.read(cx).focus_handle().is_focused(window));
            let b = find(window, &[], &ElementId::Name("top-search".into()))
                .expect("search")
                .bounds();
            point(b.origin.x - px(6.), b.origin.y + b.size.height / 2.0)
        })
        .expect("window");

    cx.simulate_keystrokes(window, "/");
    VisualTestContext::from_window(window, cx).simulate_click(wrapper_pt, Modifiers::none());
    click(cx, window, "top-search");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.input("abc", cx);
        window.render_frame(cx);
    })
    .expect("window");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        assert!(fired.borrow().is_empty(), "disabled Search must not fire");
        assert_eq!(view.read(cx).filter().as_str(), "");
        assert_eq!(window.find("top-search").value(), Some(""));
        assert_ne!(window.find("top-search").focused(), Some(true));
        assert!(view.read(cx).focus_handle().is_focused(window));
        view.update(cx, |bar, cx| bar.set_available(Action::Search, true, cx));
        window.render_frame(cx);
    })
    .expect("window");

    cx.simulate_keystrokes(window, "/");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        assert_eq!(fired.borrow().as_slice(), [Action::Search]);
        assert_eq!(window.find("top-search").focused(), Some(true));
        window.input("abc", cx);
        window.render_frame(cx);
    })
    .expect("window");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        assert_eq!(view.read(cx).filter().as_str(), "abc");
        assert_eq!(window.find("top-search").value(), Some("abc"));
    })
    .expect("window");
}

/// Hosts `Topbar` inside a fixed-width parent inside a wider window viewport.
struct BoundedHost {
    width: gpui_kit::Pixels,
    bar: Entity<Topbar>,
}

impl Render for BoundedHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(self.width).child(self.bar.clone())
    }
}

/// Settles layout/prepaint frames until own-width compact transitions finish.
fn settle(cx: &mut TestAppContext, window: AnyWindowHandle) {
    for _ in 0..2 {
        cx.update_window(window, |_, window, cx| window.render_frame(cx))
            .expect("window");
        cx.run_until_parked();
    }
}

/// Verifies one observed element is visible, positive-sized, and inside `[min_x, max_x]`.
fn assert_snap_in_bar(
    window: &Window,
    id: &str,
    min_x: gpui_kit::Pixels,
    max_x: gpui_kit::Pixels,
) -> Bounds<gpui_kit::Pixels> {
    let snap = find(window, &[], &ElementId::Name(id.into()))
        .unwrap_or_else(|| panic!("missing {id} inside [{min_x:?}, {max_x:?}]"));
    assert!(snap.visible(), "{id} must be visible");
    let b = snap.bounds();
    assert!(
        b.size.width > px(0.) && b.size.height > px(0.),
        "{id} must have positive bounds: {b:?}"
    );
    assert!(b.origin.x >= min_x, "{id} starts left of 16px inset: {b:?}");
    assert!(
        b.origin.x + b.size.width <= max_x,
        "{id} clipped past {max_x:?}: {b:?}"
    );
    b
}

/// Asserts every expected ID and visible descendant stays within `[16, width - 16]`.
fn assert_compact_fit(window: &Window, width: f32, expected: &[&str]) {
    let min_x = DIMS.inset_x;
    let max_x = px(width) - DIMS.inset_x;
    for id in expected {
        assert_snap_in_bar(window, id, min_x, max_x);
    }
    for snap in snapshots(window) {
        let b = snap.bounds();
        if !snap.visible() || b.size.width <= px(0.) {
            continue;
        }
        assert!(
            b.origin.x >= min_x,
            "{:?} starts left of 16px inset: {b:?}",
            snap.path()
        );
        assert!(
            b.origin.x + b.size.width <= max_x,
            "{:?} clipped past {max_x:?}: {b:?}",
            snap.path()
        );
    }
}

/// Asserts each visible text string appears once, fits inside `[16, width - 16]`, and fits `parent`.
fn assert_single_text(window: &Window, text: &str, parent: Option<&str>, width: f32) {
    let min_x = DIMS.inset_x;
    let max_x = px(width) - DIMS.inset_x;
    let matches: Vec<_> = texts(window)
        .into_iter()
        .filter(|entry| entry.visible() && entry.text() == text)
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected {text:?} once as visible text, found {}",
        matches.len()
    );
    let tb = matches[0].bounds();
    assert!(
        tb.size.width > px(0.) && tb.size.height > px(0.),
        "{text:?} needs positive text bounds: {tb:?}"
    );
    assert!(tb.origin.x >= min_x, "{text:?} left of inset: {tb:?}");
    assert!(
        tb.origin.x + tb.size.width <= max_x,
        "{text:?} exceeds bar right inset {max_x:?}: {tb:?}"
    );
    if let Some(pid) = parent {
        let pb = assert_snap_in_bar(window, pid, min_x, max_x);
        assert!(
            tb.origin.x >= pb.origin.x
                && tb.origin.x + tb.size.width <= pb.origin.x + pb.size.width,
            "{text:?} ({tb:?}) escapes parent {pid} ({pb:?})"
        );
    }
}

/// Verifies breadcrumb Role::Label, full accessible text, and bounded single-line geometry.
fn assert_crumb_contract(window: &Window, width: f32, compact: bool) {
    let crumbs =
        find(window, &[], &ElementId::Name("top-crumbs".into())).expect("top-crumbs must exist");
    assert_eq!(crumbs.role(), Some(Role::Label));
    assert_eq!(
        crumbs.label(),
        Some("New comparison Folder · File"),
        "crumb accessible label must always preserve complete title and detail"
    );
    assert_single_text(window, "New comparison", Some("top-crumbs"), width);
    let detail_count = texts(window)
        .iter()
        .filter(|e| e.text() == "Folder · File")
        .count();
    if compact {
        assert_eq!(detail_count, 0, "compact mode hides secondary crumb detail");
    } else {
        assert_single_text(window, "Folder · File", Some("top-crumbs"), width);
    }
}

/// Asserts enabled pill labels, slash, hints, and breadcrumb contract against `texts(window)`.
fn assert_owned_texts(window: &Window, width: f32, expect_search: bool, expected_hints: &[&str]) {
    let labels = if expect_search {
        FOLDER_LABELS
    } else {
        &FOLDER_LABELS[..3]
    };
    for (pid, label) in labels {
        assert_single_text(window, label, Some(pid), width);
    }
    let slash_count = texts(window)
        .iter()
        .filter(|entry| entry.visible() && entry.text() == "/")
        .count();
    if expect_search {
        assert_single_text(window, "/", None, width);
    } else {
        assert_eq!(slash_count, 0, "unexpected / slash without Search pill");
    }
    for hint in ALL_HINTS {
        if expected_hints.contains(hint) {
            assert_single_text(window, hint, None, width);
        } else {
            let count = texts(window).iter().filter(|e| e.text() == *hint).count();
            assert_eq!(count, 0, "compact hint {hint:?} should be hidden");
        }
    }
    assert_crumb_contract(window, width, expected_hints.is_empty());
}

/// Formats observed owned text, indicator, and opaque Input bounds into `out`.
fn append_bounds_report(out: &mut String, scenario: &str, window: &Window) {
    let _ = writeln!(out, "=== {scenario} ===");
    for t in texts(window) {
        let b = t.bounds();
        let owner = t
            .path()
            .iter()
            .rev()
            .find_map(|id| find(window, &[], id).map(|s| (id, s.bounds())));
        let owner_name = match owner {
            Some((ElementId::Name(name), _)) => name.as_ref(),
            _ => "-",
        };
        let _ = writeln!(
            out,
            "TEXT {:?} vis={} x={:?}..{:?} owner={} owner_b={:?}",
            t.text(),
            t.visible(),
            b.origin.x,
            b.origin.x + b.size.width,
            owner_name,
            owner.map(|(_, r)| (r.origin.x, r.origin.x + r.size.width))
        );
    }
    for id in ["top-theme-swatch", "top-accounts-status", "top-search"] {
        if let Some(s) = find(window, &[], &ElementId::Name(id.into())) {
            let b = s.bounds();
            let note = if id == "top-search" {
                "opaque-kit-input"
            } else {
                "owned-indicator"
            };
            let _ = writeln!(
                out,
                "{note} {id} vis={} x={:?}..{:?}",
                s.visible(),
                b.origin.x,
                b.origin.x + b.size.width
            );
        }
    }
}

const START_IDS: &[&str] = &["top-crumbs", "top-theme", "top-view", "top-keys"];
const FOLDER_IDS: &[&str] = &[
    "top-crumbs",
    "top-search",
    "top-theme",
    "top-view",
    "top-keys",
    "top-sync",
];
const FOLDER_LABELS: &[(&str, &str)] = &[
    ("top-theme", "Theme"),
    ("top-view", "View"),
    ("top-keys", "Keys"),
    ("top-sync", "Sync"),
];
const ALL_HINTS: &[&str] = &["Alt+T", "Alt+V", "?", "Alt+S"];

/// At 500 px (780 px window - 280 px Sidebar), Start and Folders fit in both Light and Dark.
#[gpui_kit::test]
fn narrow_500px_fits_start_and_folders_in_light_and_dark(cx: &mut TestAppContext) {
    let mut log = String::new();
    for theme in [Mode::Light, Mode::Dark] {
        for (mode, expected, expect_search) in [
            (TopbarMode::Start, START_IDS, false),
            (TopbarMode::Folders, FOLDER_IDS, true),
        ] {
            let (window, view, _) = topbar_in(cx, 500., theme);
            cx.update_window(window, |_, window, cx| {
                view.update(cx, |bar, cx| bar.set_mode(mode, cx));
                window.render_frame(cx);
            })
            .expect("window");
            settle(cx, window);
            cx.update_window(window, |_, window, _| {
                append_bounds_report(&mut log, &format!("500px {mode:?} {theme:?}"), window);
                assert_compact_fit(window, 500., expected);
                assert_owned_texts(window, 500., expect_search, &[]);
            })
            .expect("window");
        }
    }
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../target/c2b-owned-text-bounds.log"
    );
    // The evidence log lives in the repo target dir, which a custom CARGO_TARGET_DIR leaves absent.
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir).expect("create bounds log dir");
    }
    std::fs::write(path, log).expect("write c2b bounds log");
}

/// Builds a `Topbar` entity with default comparison crumbs for host-width tests.
fn host_topbar(window: &mut Window, cx: &mut gpui_kit::App) -> Entity<Topbar> {
    let crumbs = Crumbs {
        title: "New comparison".into(),
        detail: "Folder · File".into(),
    };
    cx.new(|cx| Topbar::new(crumbs, |_, _| {}, window, cx))
}

/// Opens a bounded `Topbar` host inside a viewport of `viewport_w`.
fn open_bounded_host(
    cx: &mut TestAppContext,
    viewport_w: f32,
    host_w: f32,
) -> (AnyWindowHandle, Entity<BoundedHost>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        apply_sidewise(PaletteId::Violet, Mode::Light, cx);
        bind_keys(cx);
    });
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: Point::default(),
            size: size(px(viewport_w), px(600.)),
        })),
        ..Default::default()
    };
    cx.update(|cx| {
        gpui_kit::open_window(options, cx, |window, cx| {
            let bar = host_topbar(window, cx);
            cx.new(|_| BoundedHost {
                width: px(host_w),
                bar,
            })
        })
        .expect("open bounded host window")
    })
}

/// Placing Topbar in a 500 px parent inside a 1200 px viewport compacts via own-width bounds.
#[gpui_kit::test]
fn own_width_500px_parent_in_1200px_viewport_compacts_from_bounds(cx: &mut TestAppContext) {
    let (window, host) = open_bounded_host(cx, 1200., 1200.);
    settle(cx, window);
    let wide_w = cx
        .update_window(window, |_, window, _| {
            assert_owned_texts(window, 1200., true, ALL_HINTS);
            width_of(window, "top-view")
        })
        .expect("window");

    cx.update(|cx| {
        host.update(cx, |h, cx| {
            h.width = px(500.);
            cx.notify();
        });
    });
    settle(cx, window);
    cx.update_window(window, |_, window, _| {
        let narrow_w = width_of(window, "top-view");
        assert!(
            narrow_w < wide_w,
            "500 px parent bounds must hide hints even in 1200 px viewport (wide={wide_w:?}, narrow={narrow_w:?})"
        );
        assert_compact_fit(window, 500., FOLDER_IDS);
        assert_owned_texts(window, 500., true, &[]);
    })
    .expect("window");
}

/// Resizing the window 780 -> 500 -> 780 toggles pill key hints across the 640 px threshold.
#[gpui_kit::test]
fn window_resize_780_500_780_toggles_hints_around_640(cx: &mut TestAppContext) {
    let (window, _view, _) = topbar(cx, MIN_W);
    settle(cx, window);
    let view_w_780 = cx
        .update_window(window, |_, window, _| {
            assert_compact_fit(window, MIN_W, FOLDER_IDS);
            assert_owned_texts(window, MIN_W, true, ALL_HINTS);
            width_of(window, "top-view")
        })
        .expect("window");

    cx.simulate_window_resize(window, size(px(500.), px(600.)));
    settle(cx, window);
    cx.update_window(window, |_, window, _| {
        assert!(
            width_of(window, "top-view") < view_w_780,
            "500 px drops hint"
        );
        assert_owned_texts(window, 500., true, &[]);
    })
    .expect("window");

    cx.simulate_window_resize(window, size(px(MIN_W), px(600.)));
    settle(cx, window);
    cx.update_window(window, |_, window, _| {
        assert_eq!(
            width_of(window, "top-view"),
            view_w_780,
            "780 px restores hint"
        );
        assert_owned_texts(window, MIN_W, true, ALL_HINTS);
    })
    .expect("window");
}
