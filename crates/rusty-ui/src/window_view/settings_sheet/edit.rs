use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::InputEvent;
use gpui_kit::{Focusable, Role, Subscription};

use crate::prelude::*;
use crate::primitives::{TextField, TextFieldState, text_field_state};
use crate::settings::{CancelEdit, Change, Control, FIELD_CONTEXT, Field, Value};
use crate::tokens::Theme;
use crate::window_view::FerriteWindow;

const NUMBER_PROBLEM: &str = "Type a whole number from 1 to 65535.";

pub(super) struct TextEdit {
    field: Field,
    input: Entity<TextFieldState>,
    error: Option<SharedString>,
    _enter: Subscription,
}

fn parsed(field: Field, text: &str) -> Result<Value, &'static str> {
    let text = text.trim();
    match field.spec().control {
        Control::Number => match text.parse::<u16>() {
            Ok(number) if number > 0 => Ok(Value::Number(number.into())),
            _ => Err(NUMBER_PROBLEM),
        },
        _ => Ok(Value::Text(text.to_owned().into())),
    }
}

impl FerriteWindow {
    pub(super) fn start_edit(&mut self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        let current = match self.values.get(field) {
            Value::Text(text) => text.to_string(),
            Value::Number(number) => number.to_string(),
            _ => return,
        };
        let input = text_field_state(field.spec().label, window, cx);
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
            Err(problem) => edit.error = Some(problem.into()),
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
            .child(TextField::new("settings-input", &edit.input, spec.label))
            .children(error);
        Some(bar.into_any_element())
    }
}
