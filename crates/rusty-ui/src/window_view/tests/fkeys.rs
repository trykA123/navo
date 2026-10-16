use std::rc::Rc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Modifiers, TestAppContext, VisualTestContext};

use super::open;

fn key_labels(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
) -> (Vec<String>, Vec<Option<bool>>) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let bar = window.within("fkeys");
        let labels = (0..10usize)
            .map(|i| bar.find(("fkey", i)).label().unwrap_or_default().to_owned())
            .collect();
        let lit = (0..3usize)
            .map(|i| bar.find(("fkey-mod", i)).selected())
            .collect();
        (labels, lit)
    })
    .expect("window")
}

fn hold(cx: &mut TestAppContext, window: AnyWindowHandle, modifiers: Modifiers) {
    let mut visual = VisualTestContext::from_window(window, cx);
    if !visual.update(|window, _| window.is_window_active()) {
        visual.deactivate_window();
        visual.update(|window, _| window.activate_window());
        visual.run_until_parked();
    }
    visual.simulate_modifiers_change(modifiers);
    visual.run_until_parked();
}

#[gpui_kit::test]
fn holding_shift_lights_its_cap_and_shows_the_shift_layer(cx: &mut TestAppContext) {
    let (window, _) = open(cx, Rc::default());
    let (base, lit) = key_labels(cx, window);
    assert_eq!(base[0], "F1");
    assert_eq!(base[7], "F8 Delete");
    assert_eq!(lit, [Some(false); 3]);
    hold(
        cx,
        window,
        Modifiers {
            shift: true,
            ..Modifiers::default()
        },
    );
    let (shift, lit) = key_labels(cx, window);
    assert_eq!(shift[3], "F4 Edit with");
    assert_eq!(shift[7], "F8 Delete for good");
    assert_eq!(lit, [Some(true), Some(false), Some(false)]);
    hold(cx, window, Modifiers::default());
    assert_eq!(key_labels(cx, window).0, base);
}

#[gpui_kit::test]
fn ctrl_and_alt_show_the_sort_and_drive_keys(cx: &mut TestAppContext) {
    let (window, _) = open(cx, Rc::default());
    hold(
        cx,
        window,
        Modifiers {
            control: true,
            ..Modifiers::default()
        },
    );
    let (ctrl, lit) = key_labels(cx, window);
    assert_eq!(
        ctrl[2..6],
        ["F3 By name", "F4 By ext", "F5 By date", "F6 By size"]
    );
    assert_eq!(lit, [Some(false), Some(true), Some(false)]);
    hold(
        cx,
        window,
        Modifiers {
            alt: true,
            ..Modifiers::default()
        },
    );
    let (alt, lit) = key_labels(cx, window);
    assert_eq!(alt[..2], ["F1 Left drive", "F2 Right drive"]);
    assert_eq!(lit, [Some(false), Some(false), Some(true)]);
}

#[gpui_kit::test]
fn the_main_window_keeps_the_pill_slot_after_f10(cx: &mut TestAppContext) {
    let (window, _) = open(cx, Rc::default());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let bar = window.within("fkeys");
        let f10 = bar.find(("fkey", 9usize));
        let slot = bar.find("fkey-pill-slot");
        assert_eq!(f10.label(), Some("F10 Palette"));
        assert!(slot.bounds().left() >= f10.bounds().right());
        assert!(bar.try_find(("fkey", 10usize)).is_none());
    })
    .expect("window");
}

#[gpui_kit::test]
fn leaving_the_window_with_alt_held_shows_the_base_layer(cx: &mut TestAppContext) {
    let (window, _) = open(cx, Rc::default());
    let (base, _) = key_labels(cx, window);
    hold(
        cx,
        window,
        Modifiers {
            alt: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(key_labels(cx, window).0[0], "F1 Left drive");
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.deactivate_window();
    let (labels, lit) = key_labels(cx, window);
    assert_eq!(labels, base);
    assert_eq!(lit, [Some(false); 3]);
}
