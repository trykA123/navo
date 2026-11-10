//! The Setup window's keys and its Folder field: Enter, Esc, Alt+letter and typed folders.
//!
//! The pure mapping lives in `navo_ui::setup::keys`; this file reads gpui's events and focus.

use std::path::PathBuf;

use gpui_kit::component::input::InputEvent;
use gpui_kit::{
    App, Context, Entity, FocusHandle, Focusable, KeyDownEvent, Keystroke, ModifiersChangedEvent,
    Subscription, Window,
};
use navo_ui::primitives::access_label::plain_text;
use navo_ui::primitives::{TextFieldState, text_field_state};
use navo_ui::setup::Strings;
use navo_ui::setup::keys::{Focus, SetupKey, footer, key_action};

use crate::flow::Event;
use crate::window::SetupWindow;
use crate::window_core::Shell;

impl SetupWindow {
    /// Builds the Folder field with the flow's folder; each edit feeds the debounced check.
    pub(crate) fn folder_field(
        shell: &Shell,
        strings: &Strings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<TextFieldState>, Subscription) {
        let field = text_field_state(plain_text(&strings.folder_label), window, cx);
        let text = shell.view().folder_text;
        field.update(cx, |state, cx| state.set_value(text, window, cx));
        let changes = cx.subscribe(&field, |view, field, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let text = field.read(cx).value().to_string();
                view.typed(text, cx);
            }
        });
        (field, changes)
    }

    /// Feeds a typed folder to the flow; text equal to the flow's folder changes nothing.
    fn typed(&mut self, text: String, cx: &mut Context<Self>) {
        if text != self.shell.view().folder_text {
            self.handle(Event::SetFolder(PathBuf::from(text)), cx);
        }
    }

    /// Takes a parent folder from the picker and shows the resulting `<parent>\Navo` in the field.
    pub fn picked(&mut self, parent: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.handle(Event::BrowsePicked(parent), cx);
        let text = self.shell.view().folder_text;
        // set_value emits no Change event, so the picked folder is not checked twice.
        self.field
            .update(cx, |state, cx| state.set_value(text, window, cx));
    }

    /// Returns the Folder field's text.
    pub fn field_text(&self, cx: &App) -> String {
        self.field.read(cx).value().to_string()
    }

    /// Runs the action of Enter, Esc or Alt+letter and stops the key there.
    pub(crate) fn key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = setup_key(&event.keystroke) else {
            return;
        };
        let focus = self.focus_kind(window, cx);
        let view = self.shell.view();
        if let Some(action) = key_action(&view, &self.strings, key, focus) {
            cx.stop_propagation();
            self.act(action, cx);
        }
    }

    /// Names what has focus: the window root (or nothing), the Folder field, or a control.
    /// The primary button's focus counts as the root on pages that draw no primary button.
    fn focus_kind(&self, window: &Window, cx: &App) -> Focus {
        let view = self.shell.view();
        let drawn = footer(&view, &self.strings)
            .iter()
            .any(|button| button.primary);
        let root =
            |focused: FocusHandle| focused == self.focus || (focused == self.primary && !drawn);
        if self.field.read(cx).focus_handle(cx).is_focused(window) {
            Focus::Field
        } else if window.focused(cx).is_none_or(root) {
            Focus::Window
        } else {
            Focus::Control
        }
    }

    /// Underlines the access keys while Alt alone is held in the active window.
    pub(crate) fn modifiers_changed(
        &mut self,
        event: &ModifiersChangedEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keys = event.modifiers;
        let show = keys.alt && !keys.control && !keys.platform && window.is_window_active();
        if self.shell.set_show_keys(show) {
            cx.notify();
        }
    }
}

/// Maps a keystroke to a window key: plain Enter or Esc, or Alt with one letter or digit.
///
/// Ctrl+Alt is AltGr on Windows keyboards and types characters, so it is never an access key.
pub fn setup_key(keystroke: &Keystroke) -> Option<SetupKey> {
    let m = keystroke.modifiers;
    let plain = !m.alt && !m.control && !m.shift && !m.platform;
    match keystroke.key.as_str() {
        "enter" if plain => Some(SetupKey::Enter),
        "escape" if plain => Some(SetupKey::Escape),
        key if m.alt && !m.control && !m.platform => {
            let mut chars = key.chars();
            let letter = chars.next().filter(|c| c.is_alphanumeric())?;
            chars.next().is_none().then_some(SetupKey::Alt(letter))
        }
        _ => None,
    }
}
