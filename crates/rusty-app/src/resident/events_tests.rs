use std::time::Instant;

use rusty_ui::settings::{Change, Field, Value, Values, parse_summon_key};

use super::super::keys::{
    HOTKEY_ID, Input, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, OPEN_ID, QUIT_ID,
    event_for, hotkey_code,
};
use super::*;

fn values(keep: bool, key: &str) -> Values {
    Values::default()
        .with(&Change {
            field: Field::KeepInTray,
            value: Value::Flag(keep),
        })
        .with(&Change::text(Field::SummonKey, key))
}

#[test]
fn open_left_click_and_the_hotkey_summon_and_quit_quits() {
    let at = Instant::now();
    for input in [
        Input::Hotkey(HOTKEY_ID),
        Input::Menu(OPEN_ID),
        Input::LeftClick,
    ] {
        assert_eq!(
            event_for(input, at),
            Some(ResidentEvent::Summon(at)),
            "{input:?}"
        );
    }
    assert_eq!(
        event_for(Input::Menu(QUIT_ID), at),
        Some(ResidentEvent::Quit)
    );
}

#[test]
fn unknown_menu_ids_and_hotkeys_do_nothing() {
    let at = Instant::now();
    assert_eq!(event_for(Input::Menu("other"), at), None);
    assert_eq!(event_for(Input::Hotkey(HOTKEY_ID + 1), at), None);
}

#[test]
fn win_alt_n_registers_as_win_alt_norepeat_and_the_n_key() {
    let key = parse_summon_key("Win+Alt+N").unwrap().unwrap();
    assert_eq!(
        hotkey_code(key),
        (MOD_WIN | MOD_ALT | MOD_NOREPEAT, u32::from(b'N'))
    );
}

#[test]
fn ctrl_shift_digit_registers_the_digit_key() {
    let key = parse_summon_key("ctrl+shift+1").unwrap().unwrap();
    assert_eq!(
        hotkey_code(key),
        (MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, u32::from(b'1'))
    );
}

#[test]
fn the_defaults_start_no_tray_but_the_summon_key() {
    let config = ResidentConfig::from_values(&Values::default());
    assert!(!config.keep_in_tray);
    assert_eq!(config.summon_key, parse_summon_key("Win+Alt+N").unwrap());
    assert!(!config.is_idle());
}

#[test]
fn tray_off_and_an_empty_key_leave_the_resident_idle() {
    assert!(ResidentConfig::from_values(&values(false, "")).is_idle());
    assert!(!ResidentConfig::from_values(&values(true, "")).is_idle());
}

#[test]
fn an_invalid_key_in_the_file_registers_no_key() {
    let config = ResidentConfig::from_values(&values(true, "Win+F13"));
    assert_eq!(config.summon_key, None);
    assert!(config.keep_in_tray);
}
