use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::InputEvent;
use gpui_kit::{Focusable, Role, Subscription};

use navo_core::list_text::{join_list, split_list};
use navo_l10n::t;

use crate::prelude::*;
use crate::primitives::{TextField, TextFieldState, text_field_state};
use crate::settings::{CancelEdit, Change, Control, FIELD_CONTEXT, Field, Value, parse_summon_key};
use crate::tokens::Theme;
use crate::window_view::NavoWindow;

const NUMBER_PROBLEM: &str = "Type a whole number from 1 to 65535.";

pub(super) struct TextEdit {
    field: Field,
    input: Entity<TextFieldState>,
    error: Option<SharedString>,
    _enter: Subscription,
}

fn joined(text: &str) -> String {
    join_list(&split_list(text))
}

/// Whether a typed number fits the field's range; only the memory budget has a narrower one.
fn in_budget_range(field: Field, number: u16) -> bool {
    let (low, high) = field.number_range();
    field != Field::CompareBudgetMib || (low..=high).contains(&i64::from(number))
}

fn parsed(field: Field, text: &str) -> Result<Value, SharedString> {
    let text = text.trim();
    match field.spec().control {
        Control::Number => match text.parse::<u16>() {
            Ok(number) if number > 0 && in_budget_range(field, number) => {
                Ok(Value::Number(number.into()))
            }
            _ if field == Field::CompareBudgetMib => {
                let (low, high) = field.number_range();
                Err(t!("settings-number-range-problem", low = low, high = high).into())
            }
            _ => Err(NUMBER_PROBLEM.into()),
        },
        _ if matches!(field, Field::SummonKey | Field::ClipboardKey) => parse_summon_key(text)
            .map(|_| Value::Text(text.to_owned().into()))
            .map_err(|error| error.message().into()),
        _ if field.is_list() => Ok(Value::Text(joined(text).into())),
        _ => Ok(Value::Text(text.to_owned().into())),
    }
}

impl NavoWindow {
    pub(super) fn start_edit(&mut self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        let current = match self.values.get(field) {
            Value::Text(text) => text.to_string(),
            Value::Number(number) => number.to_string(),
            _ => return,
        };
        let input = text_field_state(field.spec().label(), window, cx);
        input.update(cx, |state, cx| state.set_value(current, window, cx));
        let enter = cx.subscribe_in(&input, window, |view, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                view.commit_edit(window, cx);
            }
        });
        window.focus(&input.read(cx).focus_handle(cx), cx);
        self.settings.edit = Some(TextEdit {
            field,
            input,
            error: None,
            _enter: enter,
        });
        cx.notify();
    }

    fn commit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = &mut self.settings.edit else {
            return;
        };
        let field = edit.field;
        match parsed(field, &edit.input.read(cx).value()) {
            Ok(value) => {
                self.settings.edit = None;
                self.focus_settings(window, cx);
                self.set_from_sheet(Change { field, value }, window, cx);
            }
            Err(problem) => edit.error = Some(problem),
        }
        cx.notify();
    }

    fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.edit = None;
        self.focus_settings(window, cx);
        cx.notify();
    }

    pub(super) fn edit_bar(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let edit = self.settings.edit.as_ref()?;
        let spec = edit.field.spec();
        let error = edit.error.clone().map(|problem| {
            div()
                .id("settings-edit-error")
                .role(Role::Alert)
                .aria_label(problem.clone())
                .test_support()
                .text_color(t.colors.mark)
                .child(problem)
        });
        let bar = div()
            .key_context(FIELD_CONTEXT)
            .on_action(cx.listener(|view, _: &CancelEdit, window, cx| view.cancel_edit(window, cx)))
            .flex()
            .flex_col()
            .flex_none()
            .gap(t.spacing.sm)
            .px(t.spacing.xl)
            .py(t.spacing.lg)
            .border_t_1()
            .border_color(t.surfaces.card_edge)
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .child(format!("Enter saves {}, Esc cancels", spec.key))
            .child(TextField::new("settings-input", &edit.input, spec.label()))
            .children(error);
        Some(bar.into_any_element())
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
}
