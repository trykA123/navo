//! The password field of the secret form. Gpui's text input keeps its text where it cannot be
//! wiped, so this field takes the typed keys itself into one `SecretText`, which wipes on drop.

use gpui_kit::{App, ClipboardEntry, ClipboardItem, FocusHandle, KeyDownEvent};

use super::SecretText;
use super::hooks::wipe_text;

pub(super) struct Password {
    text: SecretText,
    focus: FocusHandle,
    is_shown: bool,
}

impl Password {
    pub fn new(cx: &App) -> Self {
        Self {
            text: SecretText::new(String::new()),
            focus: cx.focus_handle(),
            is_shown: false,
        }
    }

    pub fn focus(&self) -> &FocusHandle {
        &self.focus
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn is_shown(&self) -> bool {
        self.is_shown
    }

    pub fn toggle_shown(&mut self) {
        self.is_shown = !self.is_shown;
    }

    pub fn text(&self) -> &str {
        self.text.expose()
    }

    /// A second wiped buffer for the draft, so a refused save keeps what was typed.
    pub fn secret(&self) -> SecretText {
        self.text.duplicate()
    }

    /// Wipes the text and hides it.
    pub fn clear(&mut self) {
        self.text.clear();
        self.is_shown = false;
    }

    /// Adds pasted text on one line. Control characters are dropped one by one, so no plain
    /// copy of the text is built.
    pub fn paste(&mut self, pasted: &str) {
        for c in pasted.chars() {
            if !c.is_control() {
                self.text.push(c);
            }
        }
    }

    /// Adds the text of a clipboard item and wipes the item's own strings. The platform's
    /// clipboard buffers stay out of reach.
    pub fn paste_item(&mut self, item: ClipboardItem) {
        for entry in item.into_entries() {
            if let ClipboardEntry::String(string) = entry {
                self.paste(&string.text);
                wipe_text(string.text);
            }
        }
    }

    /// Takes a typed character, or Backspace; Ctrl+Backspace wipes it all. False when the key
    /// is none of these.
    pub fn key(&mut self, event: &KeyDownEvent) -> bool {
        let stroke = &event.keystroke;
        let modifiers = stroke.modifiers;
        if stroke.key == "backspace" {
            if modifiers.control {
                self.text.clear();
            } else {
                self.text.pop();
            }
            return true;
        }
        let is_alt_gr = modifiers.control && modifiers.alt;
        let is_command = modifiers.control || modifiers.alt || modifiers.platform;
        match &stroke.key_char {
            Some(typed) if is_alt_gr || !is_command => {
                self.paste(typed);
                true
            }
            _ => false,
        }
    }
}
