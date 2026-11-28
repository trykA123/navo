//! Pure keystroke and paste encoder from GPUI events to terminal input byte sequences.

use gpui_kit::{Keystroke, Modifiers};
use navo_term::grid::Modes;

/// Bracketed paste start sequence (`CSI 200 ~`).
const PASTE_START: &str = "\x1b[200~";
/// Bracketed paste end sequence (`CSI 201 ~`).
const PASTE_END: &str = "\x1b[201~";

/// Encodes a GPUI [`Keystroke`] into bytes for the terminal backend, respecting `modes`.
///
/// Returns `None` for bare modifier keys, platform shortcuts, or Navo's global `Ctrl+\`` toggle.
#[must_use]
pub fn encode_key(keystroke: &Keystroke, modes: Modes) -> Option<Vec<u8>> {
    let modifiers = keystroke.modifiers;
    if modifiers.platform || is_modifier_key(&keystroke.key) || is_toggle_shortcut(keystroke) {
        return None;
    }
    if let Some(bytes) = encode_special_key(&keystroke.key, modifiers, modes) {
        return Some(bytes);
    }
    if modifiers.control && !is_alt_gr_printable(keystroke) {
        return encode_ctrl_key(keystroke);
    }
    if modifiers.alt && !modifiers.control {
        return encode_alt_key(keystroke);
    }
    encode_printable(keystroke)
}

/// Encodes pasted `text` for the terminal backend.
///
/// When `modes.bracketed_paste` is active, wraps `text` in `\x1b[200~...\x1b[201~` after
/// iteratively stripping any embedded `\x1b[201~` terminator sequences so pasted content cannot
/// break out of bracketed paste mode.
#[must_use]
pub fn encode_paste(text: &str, modes: Modes) -> Vec<u8> {
    if !modes.bracketed_paste {
        return text.as_bytes().to_vec();
    }
    let sanitized = strip_paste_end(text);
    let mut out = Vec::with_capacity(PASTE_START.len() + sanitized.len() + PASTE_END.len());
    out.extend_from_slice(PASTE_START.as_bytes());
    out.extend_from_slice(sanitized.as_bytes());
    out.extend_from_slice(PASTE_END.as_bytes());
    out
}

/// Returns whether `keystroke` is the global `Ctrl+\`` terminal drawer toggle.
#[must_use]
pub fn is_toggle_shortcut(keystroke: &Keystroke) -> bool {
    let m = keystroke.modifiers;
    keystroke.key == "`" && m.control && !m.alt && !m.platform && !m.shift
}

fn strip_paste_end(text: &str) -> String {
    let mut cleaned = text.to_owned();
    while cleaned.contains(PASTE_END) {
        cleaned = cleaned.replace(PASTE_END, "");
    }
    cleaned
}

fn is_modifier_key(key: &str) -> bool {
    matches!(
        key,
        "" | "shift" | "control" | "alt" | "platform" | "function"
    )
}

fn xterm_modifier_code(modifiers: Modifiers) -> u8 {
    1 + u8::from(modifiers.shift) + u8::from(modifiers.alt) * 2 + u8::from(modifiers.control) * 4
}

fn encode_special_key(key: &str, modifiers: Modifiers, modes: Modes) -> Option<Vec<u8>> {
    let mod_code = xterm_modifier_code(modifiers);
    match key {
        "enter" => Some(with_alt(modifiers.alt, b"\r")),
        "tab" => Some(encode_tab(modifiers)),
        "backspace" => Some(encode_backspace(modifiers)),
        "escape" => Some(with_alt(modifiers.alt, b"\x1b")),
        "space" => Some(encode_space(modifiers)),
        "up" => Some(encode_cursor(b'A', mod_code, modes.app_cursor)),
        "down" => Some(encode_cursor(b'B', mod_code, modes.app_cursor)),
        "right" => Some(encode_cursor(b'C', mod_code, modes.app_cursor)),
        "left" => Some(encode_cursor(b'D', mod_code, modes.app_cursor)),
        "home" => Some(encode_cursor(b'H', mod_code, modes.app_cursor)),
        "end" => Some(encode_cursor(b'F', mod_code, modes.app_cursor)),
        "insert" => Some(encode_tilde(2, mod_code)),
        "delete" => Some(encode_tilde(3, mod_code)),
        "pageup" => Some(encode_tilde(5, mod_code)),
        "pagedown" => Some(encode_tilde(6, mod_code)),
        _ => encode_function_key(key, mod_code),
    }
}

fn encode_tab(modifiers: Modifiers) -> Vec<u8> {
    if modifiers.shift {
        b"\x1b[Z".to_vec()
    } else {
        with_alt(modifiers.alt, b"\t")
    }
}

fn encode_backspace(modifiers: Modifiers) -> Vec<u8> {
    let base: &[u8] = if modifiers.control { b"\x08" } else { b"\x7f" };
    with_alt(modifiers.alt, base)
}

fn encode_space(modifiers: Modifiers) -> Vec<u8> {
    let base: &[u8] = if modifiers.control { b"\x00" } else { b" " };
    with_alt(modifiers.alt, base)
}

fn encode_cursor(final_byte: u8, mod_code: u8, app_cursor: bool) -> Vec<u8> {
    if mod_code == 1 {
        let prefix = if app_cursor { b'O' } else { b'[' };
        vec![0x1b, prefix, final_byte]
    } else {
        format!("\x1b[1;{mod_code}{}", char::from(final_byte)).into_bytes()
    }
}

fn encode_tilde(code: u8, mod_code: u8) -> Vec<u8> {
    if mod_code == 1 {
        format!("\x1b[{code}~").into_bytes()
    } else {
        format!("\x1b[{code};{mod_code}~").into_bytes()
    }
}

fn encode_function_key(key: &str, mod_code: u8) -> Option<Vec<u8>> {
    let ss3 = match key {
        "f1" => Some(b'P'),
        "f2" => Some(b'Q'),
        "f3" => Some(b'R'),
        "f4" => Some(b'S'),
        _ => None,
    };
    if let Some(final_byte) = ss3 {
        return Some(encode_cursor(final_byte, mod_code, true));
    }
    let tilde = match key {
        "f5" => 15,
        "f6" => 17,
        "f7" => 18,
        "f8" => 19,
        "f9" => 20,
        "f10" => 21,
        "f11" => 23,
        "f12" => 24,
        _ => return None,
    };
    Some(encode_tilde(tilde, mod_code))
}

fn encode_ctrl_key(keystroke: &Keystroke) -> Option<Vec<u8>> {
    let byte = ctrl_byte(&keystroke.key)?;
    Some(with_alt(keystroke.modifiers.alt, &[byte]))
}

fn ctrl_byte(key: &str) -> Option<u8> {
    if key.len() == 1 {
        let b = key.as_bytes()[0];
        if b.is_ascii_alphabetic() {
            return Some(b.to_ascii_lowercase() - b'a' + 1);
        }
        return match b {
            b'@' | b'2' => Some(0x00),
            b'[' => Some(0x1b),
            b'\\' => Some(0x1c),
            b']' => Some(0x1d),
            b'^' | b'6' => Some(0x1e),
            b'_' | b'-' | b'/' => Some(0x1f),
            _ => None,
        };
    }
    None
}

fn encode_alt_key(keystroke: &Keystroke) -> Option<Vec<u8>> {
    let text = printable_text(keystroke)?;
    let mut out = Vec::with_capacity(1 + text.len());
    out.push(0x1b);
    out.extend_from_slice(text.as_bytes());
    Some(out)
}

fn encode_printable(keystroke: &Keystroke) -> Option<Vec<u8>> {
    printable_text(keystroke).map(String::into_bytes)
}

fn printable_text(keystroke: &Keystroke) -> Option<String> {
    if let Some(text) = keystroke.key_char.as_deref()
        && !text.is_empty()
        && text.chars().all(|ch| !ch.is_control())
    {
        return Some(text.to_owned());
    }
    let mut chars = keystroke.key.chars();
    let ch = chars.next()?;
    if chars.next().is_some() || ch.is_control() {
        return None;
    }
    let ch = if keystroke.modifiers.shift && ch.is_ascii_lowercase() {
        ch.to_ascii_uppercase()
    } else {
        ch
    };
    Some(ch.to_string())
}

/// Detects AltGr input (`Ctrl+Alt` producing a printable non-letter character via `key_char`).
fn is_alt_gr_printable(keystroke: &Keystroke) -> bool {
    if !(keystroke.modifiers.control && keystroke.modifiers.alt) {
        return false;
    }
    keystroke.key_char.as_deref().is_some_and(|text| {
        !text.is_empty()
            && text.chars().all(|ch| !ch.is_control())
            && !text.eq_ignore_ascii_case(&keystroke.key)
    })
}

fn with_alt(alt: bool, bytes: &[u8]) -> Vec<u8> {
    if !alt {
        return bytes.to_vec();
    }
    let mut out = Vec::with_capacity(1 + bytes.len());
    out.push(0x1b);
    out.extend_from_slice(bytes);
    out
}
