use std::time::Duration;

use gpui_kit::{Modifiers, px};

use super::*;
use crate::commands::Keymap;

fn names(layer: &Layer) -> Vec<String> {
    layer
        .groups
        .iter()
        .map(|group| group.name.to_string())
        .collect()
}

fn shorts(entries: &[Entry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| entry.short.to_string())
        .collect()
}

fn commands(entries: &[Entry]) -> Vec<&'static str> {
    entries.iter().map(|entry| entry.command).collect()
}

fn group<'a>(layer: &'a Layer, name: &str) -> &'a Group {
    layer
        .groups
        .iter()
        .find(|group| group.name == name)
        .unwrap_or_else(|| panic!("no group {name} in {:?}", names(layer)))
}

fn user(text: &str) -> Keymap {
    Keymap::builtin().overlay(text).expect("user keymap").keymap
}

const DELAY: i64 = 400;

fn delay() -> Delay {
    Delay::from_ms(DELAY)
}

#[test]
fn the_ctrl_layer_has_three_groups_with_short_labels() {
    let layer = layer_in(Keymap::builtin(), Combo::CTRL);
    assert_eq!(names(&layer), ["Files", "Tabs", "Window"]);
    let files = &group(&layer, "Files").entries;
    assert_eq!(
        shorts(files),
        [
            "Copy",
            "Cut",
            "Paste",
            "Undo",
            "Mark all",
            "Unmark all",
            "Rename several",
            "Refresh"
        ]
    );
    assert_eq!(files[0].label, "Copy to clipboard");
    assert_eq!(files[0].keys, ["C"]);
    assert_eq!(
        shorts(&group(&layer, "Window").entries),
        [
            "Command center",
            "Jobs",
            "Settings",
            "Layouts",
            "Save workspace",
            "Narrower sheet",
            "Wider sheet"
        ]
    );
    assert_eq!(layer.count(), 25);
}

#[test]
fn the_alt_and_shift_layers_use_their_own_groups() {
    let alt = layer_in(Keymap::builtin(), Combo::ALT);
    assert_eq!(names(&alt), ["Go", "Files", "Window"]);
    assert_eq!(
        shorts(&group(&alt, "Go").entries),
        ["Back", "Forward", "Edit path"]
    );
    assert_eq!(
        shorts(&group(&alt, "Window").entries),
        [
            "View menu",
            "Activity",
            "New workspace",
            "Workspace 1–9",
            "Cancel preview"
        ]
    );
    let shift = layer_in(Keymap::builtin(), Combo::SHIFT);
    assert_eq!(names(&shift), ["Marks", "Files and panes"]);
    assert_eq!(
        shorts(&group(&shift, "Marks").entries),
        [
            "Mark up",
            "Mark down",
            "Mark page up",
            "Mark page down",
            "Mark to top",
            "Mark to bottom"
        ]
    );
    assert_eq!(
        shorts(&group(&shift, "Files and panes").entries),
        ["Delete for good", "Previous pane"]
    );
    let both = layer_in(Keymap::builtin(), Combo::CTRL_SHIFT);
    assert_eq!(names(&both), ["Tabs", "Panes", "Files", "Window"]);
}

#[test]
fn a_command_without_a_group_goes_by_its_prefix() {
    let keymap = user("[[bind]]\ncommand = \"workspace.list\"\nkeys = [\"ctrl-alt-shift-w\"]\n");
    let layer = layer_where(&keymap, Combo::new(true, true, true), &|_| true);
    assert_eq!(names(&layer), ["Window"]);
    assert_eq!(layer.groups[0].entries[0].short, "Workspaces");
}

#[test]
fn a_command_with_two_keys_is_one_entry_with_both_caps() {
    let layer = layer_in(Keymap::builtin(), Combo::CTRL);
    let tabs = &group(&layer, "Tabs").entries;
    let next: Vec<&Entry> = tabs.iter().filter(|e| e.command == "tab.next").collect();
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].keys, ["Tab", "PgDn"]);
}

#[test]
fn single_modifier_layers_count_the_layers_one_modifier_away() {
    let more = |combo| {
        layer_in(Keymap::builtin(), combo)
            .more
            .iter()
            .map(|more| (more.modifier, more.count))
            .collect::<Vec<_>>()
    };
    assert_eq!(more(Combo::CTRL), [("Shift", 10), ("Alt", 4)]);
    assert_eq!(more(Combo::SHIFT), [("Ctrl", 10), ("Alt", 2)]);
    assert_eq!(more(Combo::ALT), [("Shift", 2), ("Ctrl", 4)]);
    assert!(more(Combo::CTRL_SHIFT).is_empty());
    let without_crash = layer_where(Keymap::builtin(), Combo::CTRL, &|spec| {
        spec.action.is_some() && spec.name != "crash_notice.focus"
    });
    assert_eq!(without_crash.more[0].count, 9);
}

#[test]
fn f_keys_leave_the_groups_for_the_f_row() {
    let ctrl = layer_in(Keymap::builtin(), Combo::CTRL);
    assert_eq!(
        shorts(&ctrl.fkeys),
        ["By name", "By ext", "By date", "By size"]
    );
    assert_eq!(ctrl.fkeys[0].keys, ["F3"]);
    let shift = layer_in(Keymap::builtin(), Combo::SHIFT);
    let row: Vec<(String, String)> = shift
        .fkeys
        .iter()
        .map(|entry| (entry.keys[0].to_string(), entry.short.to_string()))
        .collect();
    let row: Vec<(&str, &str)> = row.iter().map(|(k, s)| (k.as_str(), s.as_str())).collect();
    assert_eq!(
        row,
        [
            ("F1", "Guide"),
            ("F4", "Edit with"),
            ("F6", "Rename"),
            ("F7", "New file"),
            ("F8", "Delete"),
            ("F10", "Context")
        ]
    );
    assert_eq!(shift.count(), 14);
    let alt = layer_in(Keymap::builtin(), Combo::ALT);
    assert_eq!(
        shorts(&alt.fkeys),
        ["Left drive", "Right drive", "Find in files"]
    );
}

#[test]
fn delete_for_good_is_danger_in_the_list_and_the_f_row() {
    let shift = layer_in(Keymap::builtin(), Combo::SHIFT);
    let danger: Vec<(String, String)> = shift
        .groups
        .iter()
        .flat_map(|group| &group.entries)
        .chain(&shift.fkeys)
        .filter(|entry| entry.is_danger)
        .map(|entry| (entry.keys[0].to_string(), entry.short.to_string()))
        .collect();
    assert_eq!(
        danger,
        [
            ("Del".to_owned(), "Delete for good".to_owned()),
            ("F8".to_owned(), "Delete".to_owned())
        ]
    );
    let ctrl = layer_in(Keymap::builtin(), Combo::CTRL);
    assert!(
        ctrl.groups
            .iter()
            .flat_map(|g| &g.entries)
            .all(|e| !e.is_danger)
    );
}

#[test]
fn a_layer_without_bindings_is_empty() {
    let layer = layer_in(Keymap::builtin(), Combo::new(true, true, true));
    assert!(layer.is_empty());
    assert_eq!(layer.count(), 0);
    assert!(layer.groups.is_empty());
}

#[test]
fn a_user_rebind_moves_the_entry_to_its_new_layer() {
    let keymap = user("[[bind]]\ncommand = \"files.paste\"\nkeys = [\"ctrl-shift-v\"]\n");
    let ctrl = layer_in(&keymap, Combo::CTRL);
    assert!(!commands(&group(&ctrl, "Files").entries).contains(&"files.paste"));
    let both = layer_in(&keymap, Combo::CTRL_SHIFT);
    let files = &group(&both, "Files").entries;
    assert_eq!(commands(files), ["files.paste", "files.unmark_all"]);
    assert_eq!(files[0].keys, ["V"]);
}

#[test]
fn keys_outside_the_file_list_are_left_out() {
    let layer = layer_in(Keymap::builtin(), Combo::CTRL);
    let all: Vec<&str> = layer
        .groups
        .iter()
        .flat_map(|group| commands(&group.entries))
        .collect();
    assert!(!all.contains(&"viewer.save"));
    assert!(!all.contains(&"preview.recount"));
    assert!(!all.contains(&"settings.undo"));
}

#[test]
fn key_caps_are_short_names() {
    let caps = [
        ("pagedown", "PgDn"),
        ("pageup", "PgUp"),
        ("delete", "Del"),
        ("left", "←"),
        ("right", "→"),
        ("f10", "F10"),
        ("c", "C"),
        (",", ","),
        ("tab", "Tab"),
        ("escape", "Esc"),
    ];
    for (key, cap) in caps {
        assert_eq!(key_cap(key), cap, "{key}");
    }
}

#[test]
fn combos_name_their_keys_in_windows_order() {
    assert_eq!(Combo::CTRL.name(), "Ctrl");
    assert_eq!(Combo::CTRL_SHIFT.name(), "Ctrl+Shift");
    assert_eq!(Combo::new(true, true, false).name(), "Ctrl+Alt");
    let none = Modifiers::default();
    assert_eq!(Combo::of(&none), None);
}

#[test]
fn the_hint_opens_once_the_delay_has_passed() {
    let mut hold = Hold::default();
    let arm = hold.modifiers(Some(Combo::CTRL), delay());
    let arm = arm.expect("a timer");
    assert_eq!(arm.delay, Duration::from_millis(400));
    assert_eq!(hold.shown(), None);
    assert!(hold.elapsed(arm.token));
    assert_eq!(hold.shown(), Some(Combo::CTRL));
}

#[test]
fn holding_the_same_set_again_does_not_restart_the_timer() {
    let mut hold = Hold::default();
    let first = hold.modifiers(Some(Combo::CTRL), delay());
    assert!(first.is_some());
    assert_eq!(hold.modifiers(Some(Combo::CTRL), delay()), None);
}

#[test]
fn a_new_set_during_the_wait_restarts_the_delay() {
    let mut hold = Hold::default();
    let first = hold.modifiers(Some(Combo::CTRL), delay()).expect("timer");
    let second = hold.modifiers(Some(Combo::ALT), delay()).expect("timer");
    assert!(!hold.elapsed(first.token));
    assert_eq!(hold.shown(), None);
    assert!(hold.elapsed(second.token));
    assert_eq!(hold.shown(), Some(Combo::ALT));
}

#[test]
fn a_chord_cancels_the_hint_until_every_modifier_is_released() {
    let mut hold = Hold::default();
    let arm = hold.modifiers(Some(Combo::CTRL), delay()).expect("timer");
    hold.key_pressed();
    assert!(!hold.elapsed(arm.token));
    assert_eq!(hold.shown(), None);
    assert_eq!(hold.modifiers(Some(Combo::CTRL_SHIFT), delay()), None);
    assert_eq!(hold.modifiers(Some(Combo::CTRL), delay()), None);
    assert_eq!(hold.shown(), None);
    assert_eq!(hold.modifiers(None, delay()), None);
    let again = hold.modifiers(Some(Combo::CTRL), delay()).expect("timer");
    assert!(hold.elapsed(again.token));
    assert_eq!(hold.shown(), Some(Combo::CTRL));
}

#[test]
fn releasing_every_modifier_hides_the_hint_at_once() {
    let mut hold = Hold::default();
    let arm = hold.modifiers(Some(Combo::CTRL), delay()).expect("timer");
    assert!(hold.elapsed(arm.token));
    assert_eq!(hold.modifiers(None, delay()), None);
    assert_eq!(hold.shown(), None);
}

#[test]
fn adding_shift_to_a_shown_ctrl_switches_without_a_new_delay() {
    let mut hold = Hold::default();
    let arm = hold.modifiers(Some(Combo::CTRL), delay()).expect("timer");
    assert!(hold.elapsed(arm.token));
    assert_eq!(hold.modifiers(Some(Combo::CTRL_SHIFT), delay()), None);
    assert_eq!(hold.shown(), Some(Combo::CTRL_SHIFT));
    assert_eq!(hold.modifiers(Some(Combo::CTRL), delay()), None);
    assert_eq!(hold.shown(), Some(Combo::CTRL));
}

#[test]
fn a_key_while_shown_keeps_the_hint() {
    let mut hold = Hold::default();
    let arm = hold.modifiers(Some(Combo::CTRL), delay()).expect("timer");
    assert!(hold.elapsed(arm.token));
    hold.key_pressed();
    assert_eq!(hold.shown(), Some(Combo::CTRL));
}

#[test]
fn cancel_hides_the_hint_and_waits_for_release() {
    let mut hold = Hold::default();
    let arm = hold.modifiers(Some(Combo::CTRL), delay()).expect("timer");
    assert!(hold.elapsed(arm.token));
    hold.cancel();
    assert_eq!(hold.shown(), None);
    assert_eq!(hold.modifiers(Some(Combo::CTRL_SHIFT), delay()), None);
    assert_eq!(hold.shown(), None);
}

#[test]
fn the_windows_key_never_opens_the_hint() {
    let mut hold = Hold::default();
    let win = Combo::of(&Modifiers {
        control: true,
        platform: true,
        ..Modifiers::default()
    });
    assert_eq!(hold.modifiers(win, delay()), None);
    assert_eq!(hold.shown(), None);
}

#[test]
fn the_delay_is_clamped_to_150_through_2000_ms() {
    assert_eq!(Delay::from_ms(400).get(), Duration::from_millis(400));
    assert_eq!(Delay::from_ms(10).get(), Duration::from_millis(150));
    assert_eq!(Delay::from_ms(-5).get(), Duration::from_millis(150));
    assert_eq!(Delay::from_ms(9000).get(), Duration::from_millis(2000));
    let mut hold = Hold::default();
    let arm = hold.modifiers(Some(Combo::CTRL), Delay::from_ms(5000));
    assert_eq!(arm.map(|arm| arm.delay), Some(Duration::from_millis(2000)));
}

#[test]
fn a_covered_cursor_row_lifts_by_whole_rows() {
    let cover = Cover {
        top: px(480.),
        gap: px(8.),
    };
    assert_eq!(rows_to_lift(px(400.), cover, px(24.)), 0);
    assert_eq!(rows_to_lift(px(472.), cover, px(24.)), 0);
    assert_eq!(rows_to_lift(px(473.), cover, px(24.)), 1);
    assert_eq!(rows_to_lift(px(500.), cover, px(24.)), 2);
}

#[test]
fn drawer_text_reads_at_aa_in_both_themes() {
    use crate::tokens::{Mode, Theme, contrast_ratio};
    for mode in [Mode::Light, Mode::Dark] {
        let t = Theme::new(mode);
        let pairs = [
            ("text", t.colors.text, t.surfaces.sheet),
            ("muted", t.colors.muted, t.surfaces.sheet),
            ("accent text", t.colors.accent_text, t.surfaces.sheet),
            ("keycap", t.colors.text, t.surfaces.field),
            ("chip", t.surfaces.keycap_lit_label, t.surfaces.keycap_lit),
        ];
        for (name, fg, bg) in pairs {
            let ratio = contrast_ratio(fg, bg);
            assert!(ratio >= 4.5, "{mode:?} {name}: {ratio}");
        }
    }
}

fn labels(entries: &[Entry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| entry.label.to_string())
        .collect()
}

#[test]
fn a_numbered_run_folds_into_one_row() {
    let layer = layer_in(Keymap::builtin(), Combo::CTRL);
    let tabs = &group(&layer, "Tabs").entries;
    assert_eq!(
        shorts(tabs),
        [
            "New tab",
            "Close tab",
            "Next tab",
            "Previous tab",
            "Tab 1–8",
            "Last tab"
        ]
    );
    let run = tabs
        .iter()
        .find(|e| e.label == "Go to tab 1–8")
        .expect("run");
    assert_eq!(run.keys, ["1–8"]);
    assert_eq!(layer.rows(), 8);
}

#[test]
fn a_run_broken_by_a_rebind_lists_its_members() {
    let keymap = user("[[bind]]\ncommand = \"tab.go_to_fourth\"\nkeys = [\"ctrl-0\"]\n");
    let layer = layer_in(&keymap, Combo::CTRL);
    let tabs = labels(&group(&layer, "Tabs").entries);
    assert!(!tabs.iter().any(|label| label.contains('–')), "{tabs:?}");
    assert!(tabs.contains(&"Go to tab 1".to_owned()));
    assert!(tabs.contains(&"Go to tab 4".to_owned()));
}

#[test]
fn a_command_without_a_handler_is_left_out() {
    let ctrl = layer_in(Keymap::builtin(), Combo::CTRL);
    let window = &group(&ctrl, "Window").entries;
    assert!(!commands(window).contains(&"window.new"));
    let wired = layer_where(Keymap::builtin(), Combo::CTRL, &|spec| {
        spec.name != "files.paste" && spec.action.is_some()
    });
    assert!(!commands(&group(&wired, "Files").entries).contains(&"files.paste"));
}

fn fit(width: f32, has_frow: bool) -> Fit {
    Fit {
        width: px(width),
        has_frow,
    }
}

#[test]
fn ctrl_shift_and_alt_cards_share_one_height_at_every_width() {
    use crate::tokens::{Mode, Theme};
    let t = Theme::new(Mode::Dark);
    let layers =
        [Combo::CTRL, Combo::SHIFT, Combo::ALT].map(|combo| layer_in(Keymap::builtin(), combo));
    for width in [700., 1036., 1836.] {
        for has_frow in [true, false] {
            let heights = layers
                .each_ref()
                .map(|layer| drawer_height(&t, layer, fit(width, has_frow)));
            assert_eq!(
                heights[0], heights[1],
                "Ctrl and Shift at {width} px, F-row {has_frow}"
            );
            assert_eq!(
                heights[0], heights[2],
                "Ctrl and Alt at {width} px, F-row {has_frow}"
            );
        }
    }
}

#[test]
fn the_row_count_grows_only_when_the_band_is_too_narrow() {
    use crate::tokens::{Mode, Theme};
    let t = Theme::new(Mode::Dark);
    let ctrl = layer_in(Keymap::builtin(), Combo::CTRL);
    assert_eq!(rows_to_fit(&t, &ctrl, px(1836.)), 4);
    assert_eq!(rows_to_fit(&t, &ctrl, px(1036.)), 4);
    assert!(rows_to_fit(&t, &ctrl, px(700.)) > 4);
    assert_eq!(rows_to_fit(&t, &ctrl, px(10.)), 7);
}

#[test]
fn the_f_row_adds_height_only_for_one_held_modifier() {
    use crate::tokens::{Mode, Theme};
    let t = Theme::new(Mode::Light);
    let ctrl = layer_in(Keymap::builtin(), Combo::CTRL);
    assert!(
        drawer_height(&t, &ctrl, fit(1036., true)) > drawer_height(&t, &ctrl, fit(1036., false))
    );
    let both = layer_in(Keymap::builtin(), Combo::CTRL_SHIFT);
    assert_eq!(
        drawer_height(&t, &both, fit(1036., true)),
        drawer_height(&t, &both, fit(1036., false))
    );
}

#[test]
fn delete_for_good_uses_the_mark_colour_and_reads_at_aa() {
    use crate::tokens::{Mode, Theme, contrast_ratio};
    let shift = layer_in(Keymap::builtin(), Combo::SHIFT);
    let delete = shift
        .fkeys
        .iter()
        .find(|entry| entry.is_danger)
        .expect("F8");
    let mark_up = &shift.groups[0].entries[0];
    for mode in [Mode::Light, Mode::Dark] {
        let t = Theme::new(mode);
        assert_eq!(super::drawer::entry_color(&t, delete), t.colors.mark);
        assert_eq!(super::drawer::entry_color(&t, mark_up), t.colors.text);
        let ratio = contrast_ratio(t.colors.mark, t.surfaces.sheet);
        assert!(ratio >= 4.5, "{mode:?} mark on sheet: {ratio}");
    }
}
