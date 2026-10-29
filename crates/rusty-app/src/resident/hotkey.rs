use std::ptr::null_mut;

use rusty_ui::settings::SummonKey;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey};

use super::keys::{HOTKEY_ID, hotkey_code};

pub(super) struct Hotkey;

impl Hotkey {
    pub(super) fn register(key: SummonKey) -> Option<Self> {
        let (modifiers, code) = hotkey_code(key);
        // SAFETY: a null window binds the hotkey to this thread's message queue; no pointers are passed.
        if unsafe { RegisterHotKey(null_mut(), HOTKEY_ID, modifiers, code) } == 0 {
            let error = std::io::Error::last_os_error();
            tracing::warn!(%error, ?key, "the summon key is taken by another app");
            return None;
        }
        Some(Self)
    }
}

impl Drop for Hotkey {
    fn drop(&mut self) {
        // SAFETY: drops on the thread that registered HOTKEY_ID with a null window.
        unsafe { UnregisterHotKey(null_mut(), HOTKEY_ID) };
    }
}

#[cfg(test)]
mod tests {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse as keys;

    use super::super::keys::{MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN};

    #[test]
    fn the_modifier_flags_match_windows() {
        assert_eq!(MOD_ALT, keys::MOD_ALT);
        assert_eq!(MOD_CONTROL, keys::MOD_CONTROL);
        assert_eq!(MOD_SHIFT, keys::MOD_SHIFT);
        assert_eq!(MOD_WIN, keys::MOD_WIN);
        assert_eq!(MOD_NOREPEAT, keys::MOD_NOREPEAT);
        assert_eq!(u32::from(b'N'), u32::from(keys::VK_N));
        assert_eq!(u32::from(b'1'), u32::from(keys::VK_1));
    }
}
