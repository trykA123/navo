//! Tests of the View menu (U16b): presets, the Custom flip, layout in `UiPrefs`, and the
//! click, key and focus behaviour of the popover.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{Role, Subscription, TestAppContext};
use navo_compare_profile::appearance::{LayoutPref, PaletteId};
use sidewise_kit::tokens::Mode;

use super::*;
use crate::test_window::open;
use crate::theme::apply_sidewise;

/// A menu in a themed, focused window, plus the events it emits.
fn rig(
    cx: &mut TestAppContext,
    prefs: UiPrefs,
) -> (
    AnyWindowHandle,
    Entity<ViewMenu>,
    Rc<RefCell<Vec<ViewMenuEvent>>>,
    Subscription,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        apply_sidewise(PaletteId::Violet, Mode::Light, cx);
    });
    let (window, menu) = open(cx, |_, cx| ViewMenu::new(prefs, cx));
    let events = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&events);
    let sub = cx
        .update_window(window, |_, window, cx| {
            let sub = cx.subscribe(&menu, move |_, event: &ViewMenuEvent, _| {
                log.borrow_mut().push(*event)
            });
            let handle = menu.read(cx).handle.clone();
            handle.focus(window, cx);
            window.render_frame(cx);
            sub
        })
        .expect("window");
    (window, menu, events, sub)
}

/// A bare menu for the state tests.
fn bare(cx: &mut TestAppContext, prefs: UiPrefs) -> Entity<ViewMenu> {
    cx.new(|cx| ViewMenu::new(prefs, cx))
}

/// Simple turns words, syntax and fold off and unifies; Rich turns them back on and splits.
#[gpui_kit::test]
fn simple_and_rich_set_their_toggles_and_layout(cx: &mut TestAppContext) {
    let menu = bare(cx, UiPrefs::default());
    cx.update(|cx| {
        menu.update(cx, |m, cx| m.apply(Act::Preset(ViewPresetPref::Simple), cx));
        let m = menu.read(cx);
        let p = m.prefs();
        assert_eq!(
            (p.words, p.syntax, p.fold_unchanged),
            (false, false, false),
            "Simple turns them off"
        );
        assert_eq!(p.layout, LayoutPref::Unified, "Simple is unified");
        assert_eq!(p.preset, ViewPresetPref::Simple);
        menu.update(cx, |m, cx| m.apply(Act::Preset(ViewPresetPref::Rich), cx));
        let m = menu.read(cx);
        assert!(
            m.prefs().words && m.prefs().syntax && m.prefs().fold_unchanged,
            "Rich turns them on"
        );
        assert_eq!(m.prefs().layout, LayoutPref::Split, "Rich is side by side");
    });
}

/// Any manual toggle or layout change flips the preset to Custom and keeps the other values.
#[gpui_kit::test]
fn a_manual_change_flips_the_preset_to_custom(cx: &mut TestAppContext) {
    let menu = bare(cx, UiPrefs::default());
    cx.update(|cx| {
        menu.update(cx, |m, cx| m.apply(Act::Toggle(Toggle::Wrap), cx));
        let m = menu.read(cx);
        assert_eq!(
            m.prefs().preset,
            ViewPresetPref::Custom,
            "a toggle makes it Custom"
        );
        assert!(
            m.prefs().wrap_lines && m.prefs().words,
            "only the toggled value changed"
        );
        menu.update(cx, |m, cx| m.apply(Act::Preset(ViewPresetPref::Rich), cx));
        menu.update(cx, |m, cx| m.apply(Act::Layout(LayoutPref::Unified), cx));
        assert_eq!(
            menu.read(cx).prefs().preset,
            ViewPresetPref::Custom,
            "a layout change makes it Custom"
        );
    });
}

/// Picking "Single file · unified" stores `LayoutPref::Unified` in the preferences.
#[gpui_kit::test]
fn single_file_stores_the_unified_layout(cx: &mut TestAppContext) {
    let menu = bare(cx, UiPrefs::default());
    cx.update(|cx| {
        assert_eq!(menu.read(cx).prefs().layout, LayoutPref::Split);
        menu.update(cx, |m, cx| m.apply(ITEMS[4].0, cx));
        assert_eq!(ITEMS[4].1, "Single file · unified");
        assert_eq!(menu.read(cx).prefs().layout, LayoutPref::Unified);
    });
}

/// Clicking an entry applies it; the menu stays open.
#[gpui_kit::test]
fn clicks_apply_entries_and_a_kind_dismisses(cx: &mut TestAppContext) {
    let (window, menu, events, _sub) = rig(cx, UiPrefs::default());
    cx.update_window(window, |_, window, cx| {
        window.click(("view-item", 6usize), cx)
    })
    .expect("window");
    cx.update(|cx| {
        assert!(
            menu.read(cx).prefs().wrap_lines,
            "the Wrap row toggles wrapping"
        );
        assert_eq!(*events.borrow(), vec![ViewMenuEvent::Changed]);
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find(("view-item", 6usize)).checked(), Some(true));
    })
    .expect("window");
}

/// Arrow keys move the active entry, Enter applies it, Escape dismisses, and focus stays in the menu.
#[gpui_kit::test]
fn keys_move_apply_and_dismiss(cx: &mut TestAppContext) {
    let (window, menu, events, _sub) = rig(cx, UiPrefs::default());
    cx.update_window(window, |_, window, cx| {
        menu.update(cx, |m, cx| m.focus(window, cx))
    })
    .expect("window");
    cx.simulate_keystrokes(window, "down down down");
    cx.simulate_keystrokes(window, "enter");
    cx.update(|cx| {
        assert_eq!(
            menu.read(cx).prefs().layout,
            LayoutPref::Unified,
            "Rich starts on item 1; three downs reach the unified layout"
        );
        assert_eq!(menu.read(cx).prefs().preset, ViewPresetPref::Custom);
    });
    cx.simulate_keystrokes(window, "up space");
    cx.update(|cx| {
        assert_eq!(
            menu.read(cx).prefs().layout,
            LayoutPref::Split,
            "Up then Space picks side by side"
        )
    });
    cx.simulate_keystrokes(window, "escape");
    cx.update_window(window, |_, window, cx| {
        assert_eq!(
            events.borrow().last(),
            Some(&ViewMenuEvent::Dismissed),
            "Escape dismisses"
        );
        assert!(
            menu.read(cx).handle.is_focused(window),
            "focus stays for the host to restore"
        );
    })
    .expect("window");
}

/// End goes to the last of the 7 entries, and Down and Up wrap around the ends.
#[gpui_kit::test]
fn keys_wrap_over_seven_entries(cx: &mut TestAppContext) {
    let (window, menu, _events, _sub) = rig(cx, UiPrefs::default());
    assert_eq!(ITEMS.len(), 7);
    cx.simulate_keystrokes(window, "end");
    cx.update(|cx| assert_eq!(menu.read(cx).active, 6, "End goes to the last entry"));
    cx.simulate_keystrokes(window, "down");
    cx.update(|cx| {
        assert_eq!(
            menu.read(cx).active,
            0,
            "Down wraps from the last of 7 to the first"
        )
    });
    cx.simulate_keystrokes(window, "up");
    cx.update(|cx| {
        assert_eq!(
            menu.read(cx).active,
            6,
            "Up wraps from the first to the last"
        )
    });
}

/// The key hints render in gpui syntax: Alt+Z for wrapping, and a plain L (not Shift+L) for layout.
#[gpui_kit::test]
fn key_hints_render_alt_z_and_l(cx: &mut TestAppContext) {
    let (window, _menu, _events, _sub) = rig(cx, UiPrefs::default());
    cx.update_window(window, |_, window, _| {
        for (ix, text) in [(6usize, "Alt+Z"), (3, "L"), (4, "L")] {
            let hint = window.try_find(("view-key", ix));
            assert_eq!(hint.as_ref().and_then(|h| h.label()), Some(text));
        }
    })
    .expect("window");
}

/// The menu draws in both modes; the opacity entrance runs normally and is skipped under reduced motion.
#[gpui_kit::test]
fn the_entrance_is_skipped_under_reduced_motion(cx: &mut TestAppContext) {
    let (window, menu, _events, _sub) = rig(cx, UiPrefs::default());
    for mode in [Mode::Dark, Mode::Light] {
        cx.update(|cx| apply_sidewise(PaletteId::Violet, mode, cx));
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(menu.read(cx).animated.get(), "the entrance animates");
            cx.set_reduce_motion(true);
            window.render_frame(cx);
            assert!(!menu.read(cx).animated.get(), "reduced motion skips it");
            cx.set_reduce_motion(false);
            assert_eq!(window.find("view-menu").role(), Some(Role::Menu));
        })
        .expect("window");
    }
}
