//! Settings value editing: typed text and numbers, key recording, and the checks before a save.

use gpui_kit::component::input::InputEvent;
use gpui_kit::{FocusHandle, Focusable, Keystroke, Subscription};

use navo_core::list_text::{join_list, split_list};
use navo_l10n::t;

use crate::prelude::*;
use crate::primitives::{TextFieldState, text_field_state};
use crate::settings::{Change, Control, Field, Value, parse_summon_key};
use crate::window_view::NavoWindow;

/// The open edit: the field, how its value is entered, and the last refusal shown under it.
pub(super) struct TextEdit {
    pub(super) field: Field,
    pub(super) entry: Entry,
    pub(super) error: Option<SharedString>,
}

/// How the value is entered: typed into a text field, or recorded from the next key combo.
pub(super) enum Entry {
    Typed {
        input: Entity<TextFieldState>,
        _enter: Subscription,
    },
    Recorded(FocusHandle),
}

fn joined(text: &str) -> String {
    join_list(&split_list(text))
}

/// Whether a field takes a global key combo, so editing records keys instead of text.
fn is_recorded(field: Field) -> bool {
    matches!(field, Field::SummonKey | Field::ClipboardKey)
}

/// Whether a typed number fits the field's range; only the memory budget has a narrower one.
fn in_budget_range(field: Field, number: u16) -> bool {
    let (low, high) = field.number_range();
    field != Field::CompareBudgetMib || (low..=high).contains(&i64::from(number))
}

/// Checks typed or recorded text for a field; the error is the short line shown under it.
fn parsed(field: Field, text: &str) -> Result<Value, SharedString> {
    let text = text.trim();
    match field.spec().control {
        Control::Number => match text.parse::<u16>() {
            Ok(number) if number > 0 && in_budget_range(field, number) => {
                Ok(Value::Number(number.into()))
            }
            _ => {
                let (low, high) = match field {
                    Field::CompareBudgetMib => field.number_range(),
                    _ => (1, i64::from(u16::MAX)),
                };
                Err(t!("settings-number-range-problem", low = low, high = high).into())
            }
        },
        _ if is_recorded(field) => parse_summon_key(text)
            .map(|_| Value::Text(text.to_owned().into()))
            .map_err(|error| error.message().into()),
        _ if field.is_list() => Ok(Value::Text(joined(text).into())),
        _ => Ok(Value::Text(text.to_owned().into())),
    }
}

/// Turns a pressed key into combo text such as "Ctrl+Alt+K"; `None` while only modifiers are down.
fn combo_text(keystroke: &Keystroke) -> Option<String> {
    let key = keystroke.key.as_str();
    if matches!(
        key,
        "control" | "alt" | "shift" | "platform" | "function" | ""
    ) {
        return None;
    }
    let held = &keystroke.modifiers;
    let names = [
        (held.platform, "Win"),
        (held.control, "Ctrl"),
        (held.alt, "Alt"),
        (held.shift, "Shift"),
    ];
    let mut parts: Vec<String> = names
        .iter()
        .filter(|(is_down, _)| *is_down)
        .map(|(_, name)| (*name).to_owned())
        .collect();
    parts.push(key.to_uppercase());
    Some(parts.join("+"))
}

impl NavoWindow {
    /// Builds a text field holding `current` that saves on Enter, and focuses it.
    fn typed_entry(
        &self,
        field: Field,
        current: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entry {
        let label = field.spec().label();
        let input = self.settings.host.with_window_or(window, cx, |target, cx| {
            let input = text_field_state(label.clone(), target, cx);
            input.update(cx, |state, cx| {
                state.set_value(current.clone(), target, cx);
            });
            target.focus(&input.read(cx).focus_handle(cx), cx);
            input
        });
        let enter = cx.subscribe_in(&input, window, |view, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                view.commit_edit(window, cx);
            }
        });
        Entry::Typed {
            input,
            _enter: enter,
        }
    }

    /// Opens the editor for a text, number or key field; other controls are ignored.
    pub(in crate::window_view) fn start_edit(
        &mut self,
        field: Field,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.values.is_enabled(field) {
            return;
        }
        let current = match self.values.get(field) {
            Value::Text(text) => text.to_string(),
            Value::Number(number) => number.to_string(),
            _ => return,
        };
        self.settings.row = field.row();
        self.settings.folds.reveal(self.settings.row);
        let entry = if is_recorded(field) {
            let focus = cx.focus_handle();
            self.settings
                .host
                .with_window_or(window, cx, |target, cx| target.focus(&focus, cx));
            Entry::Recorded(focus)
        } else {
            self.typed_entry(field, current, window, cx)
        };
        self.settings.edit = Some(TextEdit {
            field,
            entry,
            error: None,
        });
        cx.notify();
    }

    /// Saves the typed text, or keeps the edit open with the problem under the field.
    pub(super) fn commit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(TextEdit {
            entry: Entry::Typed { input, .. },
            ..
        }) = &self.settings.edit
        else {
            return;
        };
        let text = input.read(cx).value().to_string();
        self.save_edit(&text, window, cx);
    }

    /// Records a pressed combo: Esc cancels, Delete or Backspace clears, other keys save or refuse.
    pub(super) fn record_key(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_bare = keystroke.modifiers.number_of_modifiers() == 0;
        match keystroke.key.as_str() {
            "escape" if is_bare => self.cancel_edit(window, cx),
            "delete" | "backspace" if is_bare => self.save_edit("", window, cx),
            _ => {
                if let Some(text) = combo_text(keystroke) {
                    self.save_edit(&text, window, cx);
                }
            }
        }
    }

    fn save_edit(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = &mut self.settings.edit else {
            return;
        };
        let field = edit.field;
        match parsed(field, text) {
            Ok(value) => {
                self.settings.edit = None;
                self.focus_settings(window, cx);
                self.set_from_sheet(Change { field, value }, window, cx);
            }
            Err(problem) => edit.error = Some(problem),
        }
        cx.notify();
    }

    pub(super) fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.edit = None;
        self.focus_settings(window, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bad_clipboard_key_shows_the_parser_message() {
        let message = parsed(Field::ClipboardKey, "N").err();
        assert_eq!(
            message,
            Some(SharedString::from(
                crate::settings::SummonKeyError::NoModifier.message()
            ))
        );
        assert!(parsed(Field::ClipboardKey, "Win+Alt+V").is_ok());
        assert!(parsed(Field::ClipboardKey, "").is_ok());
    }

    #[test]
    fn a_typed_memory_budget_must_be_in_the_stores_range() {
        let typed = |text| parsed(Field::CompareBudgetMib, text);
        assert_eq!(typed("300").ok(), Some(Value::Number(300)));
        assert_eq!(typed("256").ok(), Some(Value::Number(256)));
        assert_eq!(typed("16384").ok(), Some(Value::Number(16_384)));
        assert!(typed("255").is_err());
        assert!(typed("16385").is_err());
        let problem = typed("9").expect_err("a problem");
        assert!(
            problem.contains("256") && problem.contains("16384"),
            "{problem}"
        );
        assert!(parsed(Field::UiScale, "1").is_ok());
    }

    #[test]
    fn a_bad_font_size_names_the_whole_range() {
        let problem = parsed(Field::FontSize, "big").expect_err("a problem");
        assert_eq!(problem.as_ref(), "Type a whole number from 1 to 65535.");
    }

    #[test]
    fn a_recorded_combo_reads_like_the_typed_form() {
        let combo = |text| combo_text(&Keystroke::parse(text).expect("keystroke"));
        assert_eq!(combo("ctrl-alt-k").as_deref(), Some("Ctrl+Alt+K"));
        assert_eq!(combo("super-shift-1").as_deref(), Some("Win+Shift+1"));
        assert_eq!(combo("k").as_deref(), Some("K"));
        assert_eq!(combo("ctrl-control"), None);
    }
}
