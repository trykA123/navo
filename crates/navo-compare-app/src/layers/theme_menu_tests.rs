//! Tests of the Theme picker's state (TH4): the start row, palette and mode picks, and silence
//! on a pick that changes nothing.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{KeyDownEvent, Keystroke, Role, Subscription, TestAppContext, VisualTestContext};
use sidewise_kit::tokens::Mode;

use super::*;
use crate::test_window::open;
use crate::theme::apply_sidewise;

/// A picker over `palette` and `mode`, plus the events it emits.
fn rig(
    cx: &mut TestAppContext,
    palette: PaletteId,
    mode: AppearanceMode,
) -> (
    Entity<ThemeMenu>,
    Rc<RefCell<Vec<ThemeMenuEvent>>>,
    Subscription,
) {
    cx.update(gpui_kit::init);
    let menu = cx.new(|cx| ThemeMenu::new(palette, mode, cx));
    let events = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&events);
    let sub = cx.update(|cx| {
        cx.subscribe(&menu, move |_, event: &ThemeMenuEvent, _| {
            log.borrow_mut().push(*event)
        })
    });
    (menu, events, sub)
}

/// A new picker starts on its palette's row.
#[gpui_kit::test]
fn a_new_menu_starts_on_the_current_palette(cx: &mut TestAppContext) {
    let (menu, _events, _sub) = rig(cx, PaletteId::Ocean, AppearanceMode::System);
    assert_eq!(menu.read_with(cx, |m, _| m.active), 2);
}

/// Choosing another palette changes it and emits one `Changed`.
#[gpui_kit::test]
fn choosing_a_palette_changes_it_and_says_so(cx: &mut TestAppContext) {
    let (menu, events, _sub) = rig(cx, PaletteId::Violet, AppearanceMode::System);
    menu.update(cx, |m, cx| m.apply(Pick::Palette(PaletteId::Ember), cx));
    assert_eq!(
        menu.read_with(cx, |m, _| m.choice()),
        (PaletteId::Ember, AppearanceMode::System)
    );
    assert_eq!(*events.borrow(), vec![ThemeMenuEvent::Changed]);
}

/// Choosing a mode leaves the palette alone.
#[gpui_kit::test]
fn choosing_a_mode_keeps_the_palette(cx: &mut TestAppContext) {
    let (menu, events, _sub) = rig(cx, PaletteId::Graphite, AppearanceMode::System);
    menu.update(cx, |m, cx| m.apply(Pick::Mode(AppearanceMode::Dark), cx));
    assert_eq!(
        menu.read_with(cx, |m, _| m.choice()),
        (PaletteId::Graphite, AppearanceMode::Dark)
    );
    assert_eq!(*events.borrow(), vec![ThemeMenuEvent::Changed]);
}

/// Choosing what is already chosen emits nothing.
#[gpui_kit::test]
fn choosing_the_current_value_is_silent(cx: &mut TestAppContext) {
    let (menu, events, _sub) = rig(cx, PaletteId::Ocean, AppearanceMode::Light);
    menu.update(cx, |m, cx| {
        m.apply(Pick::Palette(PaletteId::Ocean), cx);
        m.apply(Pick::Mode(AppearanceMode::Light), cx);
    });
    assert!(events.borrow().is_empty());
}

/// A picker in a themed, focused window, plus the events it emits.
fn window_rig(
    cx: &mut TestAppContext,
) -> (
    AnyWindowHandle,
    Entity<ThemeMenu>,
    Rc<RefCell<Vec<ThemeMenuEvent>>>,
    Subscription,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        apply_sidewise(PaletteId::Violet, Mode::Light, cx);
    });
    let (window, menu) = open(cx, |_, cx| {
        ThemeMenu::new(PaletteId::Violet, AppearanceMode::System, cx)
    });
    let events = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&events);
    let sub = cx
        .update_window(window, |_, window, cx| {
            let sub = cx.subscribe(&menu, move |_, event: &ThemeMenuEvent, _| {
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

/// Arrow keys move the active row and change nothing (NC153: commit-only).
#[gpui_kit::test]
fn arrows_move_without_changing_the_theme(cx: &mut TestAppContext) {
    let (window, menu, events, _sub) = window_rig(cx);
    cx.simulate_keystrokes(window, "down down");
    assert!(events.borrow().is_empty());
    assert_eq!(
        menu.read_with(cx, |m, _| (m.active, m.palette)),
        (2, PaletteId::Violet)
    );
}

/// Enter applies the active row.
#[gpui_kit::test]
fn enter_chooses_the_active_row(cx: &mut TestAppContext) {
    let (window, menu, events, _sub) = window_rig(cx);
    cx.simulate_keystrokes(window, "down enter");
    assert_eq!(menu.read_with(cx, |m, _| m.palette), PaletteId::Graphite);
    assert_eq!(*events.borrow(), vec![ThemeMenuEvent::Changed]);
}

/// A held Enter (key repeat) does not pick again or at all.
#[gpui_kit::test]
fn a_held_enter_is_ignored(cx: &mut TestAppContext) {
    let (window, menu, events, _sub) = window_rig(cx);
    cx.simulate_keystrokes(window, "down");
    let mut vcx = VisualTestContext::from_window(window, cx);
    vcx.simulate_event(KeyDownEvent {
        keystroke: Keystroke::parse("enter").expect("key"),
        is_held: true,
        prefer_character_input: false,
    });
    assert!(events.borrow().is_empty());
    assert_eq!(menu.read_with(cx, |m, _| m.palette), PaletteId::Violet);
}

/// Escape asks the host to close the picker.
#[gpui_kit::test]
fn escape_dismisses(cx: &mut TestAppContext) {
    let (window, _menu, events, _sub) = window_rig(cx);
    cx.simulate_keystrokes(window, "escape");
    assert_eq!(*events.borrow(), vec![ThemeMenuEvent::Dismissed]);
}

/// Clicking the Light row sets Light.
#[gpui_kit::test]
fn clicking_light_sets_light(cx: &mut TestAppContext) {
    let (window, menu, events, _sub) = window_rig(cx);
    cx.update_window(window, |_, window, cx| {
        window.click(("theme-item", 6usize), cx)
    })
    .expect("window");
    assert_eq!(menu.read_with(cx, |m, _| m.mode), AppearanceMode::Light);
    assert_eq!(*events.borrow(), vec![ThemeMenuEvent::Changed]);
}

/// Eight radio rows, labelled in `ITEMS` order, in a labelled menu.
#[gpui_kit::test]
fn the_rows_have_radio_roles_and_labels(cx: &mut TestAppContext) {
    let (window, _menu, _events, _sub) = window_rig(cx);
    cx.update_window(window, |_, window, _| {
        assert_eq!(window.find("theme-menu").role(), Some(Role::Menu));
        for (ix, (_, label)) in ITEMS.iter().enumerate() {
            let row = window.find(("theme-item", ix));
            assert_eq!(row.role(), Some(Role::MenuItemRadio));
            assert_eq!(row.label(), Some(*label));
        }
    })
    .expect("window");
}

/// The High contrast row's tag reads exactly "AAA", not a key name.
#[gpui_kit::test]
fn the_high_contrast_tag_reads_aaa(cx: &mut TestAppContext) {
    let (window, _menu, _events, _sub) = window_rig(cx);
    cx.update_window(window, |_, window, _| {
        let tag = window.find(("theme-key", 4usize));
        assert_eq!(tag.label(), Some("AAA contrast"));
        let found = gpui_kit::base::test_support::texts(window)
            .into_iter()
            .any(|t| t.visible() && t.text() == "AAA");
        assert!(found, "no visible text reads AAA");
    })
    .expect("window");
}
