use std::rc::Rc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Modifiers, TestAppContext, VisualTestContext};

use super::{fake, home_beside_dest, open, open_with, press};
use crate::commands::Keymap;
use crate::fkey_bar::KeyLayer;
use crate::window_view::{
    delete_warning_in, key_labels_in, keys_in_layer, layer_keys_in, settings_labels_in,
};

pub(super) fn key_labels(
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

pub(super) fn hold(cx: &mut TestAppContext, window: AnyWindowHandle, modifiers: Modifiers) {
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
fn the_main_window_reserves_no_pill_area_without_a_job(cx: &mut TestAppContext) {
    let (window, _) = open(cx, Rc::default());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let bar = window.within("fkeys");
        assert!(bar.try_find("fkey-pill-slot").is_none());
        assert!(bar.try_find(("fkey", 10usize)).is_none());
    })
    .expect("window");
}

#[gpui_kit::test]
fn a_copy_job_reserves_the_pill_area_after_f10(cx: &mut TestAppContext) {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let (window, view) = open_with(cx, fs, state, Rc::default());
    press(cx, window, &view, "f5");
    press(cx, window, &view, "enter");
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let bar = window.within("fkeys");
        let f10 = bar.find(("fkey", 9usize));
        let slot = bar.find("fkey-pill-slot");
        assert_eq!(f10.label(), Some("F10 Menu"));
        assert!(slot.bounds().left() >= f10.bounds().right());
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

#[test]
fn the_f_key_labels_follow_the_keymap() {
    let text = "[[bind]]\ncommand = \"files.view\"\nkeys = [\"f9\"]\n\n\
                [[bind]]\ncommand = \"files.rename\"\nkeys = [\"ctrl-f2\"]\n";
    let keymap = Keymap::parse(text).expect("keymap");
    let base = layer_keys_in(&keymap, None);
    let ctrl = layer_keys_in(&keymap, Some(KeyLayer::Ctrl));
    assert_eq!(
        (base[1], base[2], base[8]),
        (("F2", ""), ("F3", ""), ("F9", "View"))
    );
    assert_eq!((ctrl[1], ctrl[2]), (("F2", "Rename"), ("F3", "")));
    assert_eq!(key_labels_in(&keymap)[1], ["", "", "Rename", ""]);
}

#[test]
fn the_built_in_keymap_gives_the_known_f_key_bar() {
    let labels = |layer| layer_keys_in(Keymap::builtin(), layer).map(|(_, label)| label);
    let base = [
        "",
        "Rename",
        "View",
        "Edit",
        "Copy",
        "Move",
        "New folder",
        "Delete",
        "",
        "Menu",
    ];
    let shift = [
        "",
        "",
        "",
        "Edit with",
        "",
        "Rename",
        "New file",
        "Delete",
        "",
        "Context",
    ];
    let ctrl = [
        "", "", "By name", "By ext", "By date", "By size", "", "", "", "",
    ];
    let alt = ["Left drive", "Right drive", "", "", "", "", "", "", "", ""];
    assert_eq!(labels(None), base);
    assert_eq!(labels(Some(KeyLayer::Shift)), shift);
    assert_eq!(labels(Some(KeyLayer::Ctrl)), ctrl);
    assert_eq!(labels(Some(KeyLayer::Alt)), alt);
}

#[test]
fn the_delete_warning_sits_on_the_key_of_files_delete() {
    assert_eq!(
        delete_warning_in(Keymap::builtin()),
        Some((Some(KeyLayer::Shift), 7))
    );
    let cases = [
        (
            "\"shift-delete\", \"alt-f2\"",
            Some((Some(KeyLayer::Alt), 1)),
        ),
        ("\"f4\"", Some((None, 3))),
        ("\"shift-delete\"", None),
        ("", None),
    ];
    for (keys, expected) in cases {
        let text = format!("[[bind]]\ncommand = \"files.delete\"\nkeys = [{keys}]\n");
        let keymap = Keymap::parse(&text).expect("keymap");
        assert_eq!(delete_warning_in(&keymap), expected, "{keys}");
    }
}

#[gpui_kit::test]
fn f9_has_no_command_so_its_cap_is_dimmed_like_f1(cx: &mut TestAppContext) {
    let (window, _) = open(cx, Rc::default());
    let (base, _) = key_labels(cx, window);
    assert_eq!((base[0].as_str(), base[8].as_str()), ("F1", "F9"));
}

#[gpui_kit::test]
fn the_shift_layer_shows_the_context_menu_on_f10(cx: &mut TestAppContext) {
    let (window, _) = open(cx, Rc::default());
    hold(
        cx,
        window,
        Modifiers {
            shift: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(key_labels(cx, window).0[9], "F10 Context");
}

#[test]
fn the_settings_layer_comes_from_the_settings_commands_in_the_keymap() {
    let labels =
        |keymap: &Keymap| keys_in_layer(&settings_labels_in(keymap), None).map(|(_, label)| label);
    assert_eq!(
        labels(Keymap::builtin()),
        [
            "",
            "Edit value",
            "TOML line",
            "",
            "",
            "",
            "",
            "",
            "",
            "Menu"
        ]
    );
    let text = "[[bind]]\ncommand = \"settings.show_in_file\"\nkeys = [\"f7\"]\n\n\
                [[bind]]\ncommand = \"files.view\"\nkeys = [\"f3\"]\n";
    let moved = labels(&Keymap::parse(text).expect("keymap"));
    assert_eq!((moved[2], moved[6]), ("", "TOML line"));
}
