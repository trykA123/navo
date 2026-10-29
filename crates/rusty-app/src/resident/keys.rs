use std::time::Instant;

use rusty_ui::settings::SummonKey;

use super::events::ResidentEvent;

pub(crate) const OPEN_ID: &str = "navo-open";
pub(crate) const QUIT_ID: &str = "navo-quit";
pub(crate) const HOTKEY_ID: i32 = 1;

pub(crate) const MOD_ALT: u32 = 0x1;
pub(crate) const MOD_CONTROL: u32 = 0x2;
pub(crate) const MOD_SHIFT: u32 = 0x4;
pub(crate) const MOD_WIN: u32 = 0x8;
pub(crate) const MOD_NOREPEAT: u32 = 0x4000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Input<'a> {
    Hotkey(i32),
    Menu(&'a str),
    LeftClick,
}

pub(crate) fn event_for(input: Input<'_>, at: Instant) -> Option<ResidentEvent> {
    match input {
        Input::Hotkey(HOTKEY_ID) | Input::Menu(OPEN_ID) | Input::LeftClick => {
            Some(ResidentEvent::Summon(at))
        }
        Input::Menu(QUIT_ID) => Some(ResidentEvent::Quit),
        Input::Hotkey(_) | Input::Menu(_) => None,
    }
}

pub(crate) fn hotkey_code(key: SummonKey) -> (u32, u32) {
    let flags = [
        (key.alt, MOD_ALT),
        (key.ctrl, MOD_CONTROL),
        (key.shift, MOD_SHIFT),
        (key.win, MOD_WIN),
    ];
    let modifiers = flags
        .iter()
        .filter(|(on, _)| *on)
        .fold(MOD_NOREPEAT, |all, (_, flag)| all | flag);
    (modifiers, u32::from(key.key.to_ascii_uppercase()))
}
