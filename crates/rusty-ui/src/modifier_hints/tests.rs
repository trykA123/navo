use std::time::Duration;

use gpui_kit::{Modifiers, px};

use super::*;
use crate::commands::Keymap;

fn names(layer: &Layer) -> Vec<&'static str> {
    layer.groups.iter().map(|group| group.name).collect()
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
fn the_ctrl_layer_groups_follow_the_fixed_order() {
    let layer = layer_in(Keymap::builtin(), Combo::CTRL);
    assert_eq!(
        names(&layer),
        [
            "Clipboard",
            "Tabs",
            "Sort",
            "Files",
            "Pane",
            "View",
            "Window",
            "Other"
        ]
    );
    assert_eq!(
        commands(&group(&layer, "Files").entries),
        [
            "files.multi_rename",
            "files.undo",
            "files.mark_all",
            "files.unmark_all"
        ]
    );
    let clipboard = &group(&layer, "Clipboard").entries;
    assert_eq!(
        commands(clipboard),
        ["files.clipboard_copy", "files.clipboard_cut", "files.paste"]
    );
    assert_eq!(clipboard[0].label, "Copy to clipboard");
    assert_eq!(clipboard[0].keys, ["C"]);
    assert_eq!(
        commands(&group(&layer, "Sort").entries),
        [
            "files.sort_by_name",
            "files.sort_by_extension",
            "files.sort_by_date",
            "files.sort_by_size"
        ]
    );
    assert_eq!(layer.count(), 25);
}

#[test]
fn the_alt_and_shift_layers_map_their_commands_to_groups() {
    let alt = layer_in(Keymap::builtin(), Combo::ALT);
    assert_eq!(
        names(&alt),
        ["Files", "Pane", "View", "Navigation", "Window", "Other"]
    );
    assert_eq!(
        commands(&group(&alt, "Navigation").entries),
        ["pane.go_back", "pane.go_forward"]
    );
    let shift = layer_in(Keymap::builtin(), Combo::SHIFT);
    assert_eq!(names(&shift), ["Files"]);
    assert_eq!(
        commands(&group(&shift, "Files").entries),
        [
            "files.edit_with",
            "files.rename",
            "files.new_file",
            "files.delete",
            "files.extend_up",
            "files.extend_down",
            "files.extend_page_up",
            "files.extend_page_down",
            "files.extend_first",
            "files.extend_last",
            "files.context_menu"
        ]
    );
}

#[test]
fn a_command_without_a_group_goes_to_other() {
    let keymap = user("[[bind]]\ncommand = \"workspace.list\"\nkeys = [\"ctrl-alt-shift-w\"]\n");
    let layer = layer_where(&keymap, Combo::new(true, true, true), &|_| true);
    assert_eq!(names(&layer), ["Other"]);
    assert_eq!(layer.groups[0].entries[0].label, "Workspaces");
}

#[test]
fn a_command_with_two_keys_is_one_entry_with_both_caps() {
    let layer = layer_in(Keymap::builtin(), Combo::CTRL);
    let tabs = &group(&layer, "Tabs").entries;
    let next: Vec<&Entry> = tabs.iter().filter(|e| e.command == "tab.next").collect();
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].keys, ["Tab", "PgDn"]);
    let delete = layer_in(Keymap::builtin(), Combo::SHIFT);
    let files = &group(&delete, "Files").entries;
    let entry = files.iter().find(|e| e.command == "files.delete");
    assert_eq!(
        entry.map(|e| e.keys.clone()),
        Some(vec!["Del".into(), "F8".into()])
    );
}

#[test]
fn the_ctrl_layer_lists_the_ctrl_shift_keys_in_a_last_column() {
    let ctrl = layer_in(Keymap::builtin(), Combo::CTRL);
    assert_eq!(
        commands(&ctrl.more),
        [
            "tab.previous",
            "files.unmark_all",
            "pane.reset_split",
            "preview.browse",
            "tab.clone",
            "crash_notice.focus",
            "layout.focus_pane_first"
        ]
    );
    assert_eq!(ctrl.more[0].keys, ["Tab"]);
    assert!(layer_in(Keymap::builtin(), Combo::SHIFT).more.is_empty());
    assert!(
        layer_in(Keymap::builtin(), Combo::CTRL_SHIFT)
            .more
            .is_empty()
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
    assert_eq!(
        commands(&group(&ctrl, "Clipboard").entries),
        ["files.clipboard_copy", "files.clipboard_cut"]
    );
    assert!(commands(&ctrl.more).contains(&"files.paste"));
    let both = layer_in(&keymap, Combo::CTRL_SHIFT);
    assert_eq!(group(&both, "Clipboard").entries[0].keys, ["V"]);
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
        labels(tabs),
        [
            "New tab",
            "Close tab",
            "Next tab",
            "Previous tab",
            "Go to tab 1–8",
            "Go to last tab"
        ]
    );
    let run = tabs
        .iter()
        .find(|e| e.label == "Go to tab 1–8")
        .expect("run");
    assert_eq!(run.keys, ["1–8"]);
    assert_eq!(layer.rows(), 7);
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
    let alt = layer_in(Keymap::builtin(), Combo::ALT);
    assert_eq!(commands(&group(&alt, "Window").entries), ["activity.focus"]);
    let wired = layer_where(Keymap::builtin(), Combo::CTRL, &|spec| {
        spec.name != "files.paste" && spec.action.is_some()
    });
    assert!(!commands(&group(&wired, "Clipboard").entries).contains(&"files.paste"));
}
