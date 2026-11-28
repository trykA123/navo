//! Settings sheet bottom bar: the open editor, key recording, and the named status line with Undo.

use gpui_kit::base::TestSupportExt;
use gpui_kit::{KeyDownEvent, Role};
use navo_l10n::{t, tr};

use super::edit::{Entry, TextEdit};
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconName, Kbd, TextField, icon};
use crate::settings::{CancelEdit, Control, FIELD_CONTEXT, Field, Value, Values};
use crate::tokens::Theme;
use crate::window_view::NavoWindow;

/// The last thing the sheet did to a setting, named in the status line until the row moves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Status {
    Set(Field),
    Reset(Field),
    Undid(Field),
}

impl Status {
    /// Names a change from the sheet: a reset when it writes the default back.
    pub(super) fn of_change(field: Field, value: &Value) -> Self {
        match value {
            Value::Reset | Value::Unset => Self::Reset(field),
            _ => Self::Set(field),
        }
    }
}

/// The value as the status line says it: the option name, On or Off, the text, or None.
fn shown_value(values: &Values, field: Field) -> String {
    match (values.get(field), field.spec().control) {
        (Value::Pick(at), Control::Choices(options, _)) => options
            .get(*at)
            .map_or_else(String::new, |option| option.text().to_string()),
        (Value::Flag(is_on), _) => tr(if *is_on {
            "settings-value-on"
        } else {
            "settings-value-off"
        }),
        (Value::Number(number), _) => number.to_string(),
        (Value::Text(text), _) if !text.is_empty() => text.to_string(),
        _ => tr("settings-value-none"),
    }
}

/// The status sentence, such as "Density set to Compact · saved".
fn status_text(status: Status, values: &Values) -> String {
    let (id, field) = match status {
        Status::Set(field) => ("settings-status-set", field),
        Status::Reset(field) => ("settings-status-reset", field),
        Status::Undid(field) => ("settings-status-undid", field),
    };
    let setting = field.spec().label().to_string();
    t!(id, setting = setting, value = shown_value(values, field))
}

fn bar(t: &Theme) -> Div {
    div()
        .flex()
        .flex_none()
        .gap(t.spacing.sm)
        .px(t.spacing.xl)
        .py(t.spacing.sm)
        .border_t_1()
        .border_color(t.surfaces.card_edge)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
}

fn problem_line(t: &Theme, problem: SharedString) -> impl IntoElement {
    div()
        .id("settings-edit-error")
        .role(Role::Alert)
        .aria_label(problem.clone())
        .test_support()
        .text_color(t.colors.mark)
        .child(problem)
}

impl NavoWindow {
    /// The typed editor: the hint, the text field and a Save button for the mouse.
    fn typed_editor_row(
        &self,
        field: Field,
        input: &Entity<crate::primitives::TextFieldState>,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let spec = field.spec();
        let save = Button::new("settings-edit-save", tr("settings-edit-save"))
            .on_click(self.settings_action(cx, |view, window, cx| view.commit_edit(window, cx)));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .gap(t.spacing.sm)
            .child(t!("settings-edit-hint", key = spec.key))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.spacing.sm)
                    .child(div().flex_1().child(TextField::new(
                        "settings-input",
                        input,
                        spec.label(),
                    )))
                    .child(save),
            )
            .into_any_element()
    }

    /// The bottom bar: the open editor, else the status of the last change, else nothing.
    pub(super) fn edit_bar(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        match &self.settings.edit {
            Some(edit) => Some(self.editor(edit, t, cx)),
            None => self.status_line(t, cx),
        }
    }

    fn editor(&self, edit: &TextEdit, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let cancel = Button::new("settings-edit-cancel", tr("settings-edit-cancel"))
            .variant(ButtonVariant::Ghost)
            .on_click(self.settings_action(cx, |view, window, cx| view.cancel_edit(window, cx)));
        let entry = match &edit.entry {
            Entry::Typed { input, .. } => self.typed_editor_row(edit.field, input, t, cx),
            Entry::Recorded(focus) => div()
                .id("settings-record")
                .role(Role::Status)
                .aria_label(tr("settings-record-hint"))
                .test_support()
                .track_focus(focus)
                .on_key_down(self.settings_listener_stopping(
                    cx,
                    |view, event: &KeyDownEvent, window, cx| {
                        view.record_key(&event.keystroke, window, cx);
                    },
                ))
                .flex_1()
                .text_color(t.colors.accent_text)
                .child(tr("settings-record-hint"))
                .into_any_element(),
        };
        bar(t)
            .key_context(FIELD_CONTEXT)
            .on_action(self.settings_action::<CancelEdit>(cx, |view, window, cx| {
                view.cancel_edit(window, cx);
            }))
            .flex_col()
            .py(t.spacing.lg)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.spacing.sm)
                    .child(entry)
                    .child(cancel),
            )
            .children(edit.error.clone().map(|problem| problem_line(t, problem)))
            .into_any_element()
    }

    /// The named status of the last change, with Undo while there is something to undo.
    fn status_line(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let status = self
            .settings
            .status
            .filter(|_| self.saver.error.is_none())?;
        let text = status_text(status, &self.values);
        let can_undo = !self.settings.undo.is_empty();
        let undo = can_undo.then(|| {
            Button::new("settings-status-undo", tr("settings-status-undo"))
                .variant(ButtonVariant::Ghost)
                .icon(IconName::RotateCcw)
                .on_click(
                    self.settings_action(cx, |view, window, cx| view.undo_setting(window, cx)),
                )
        });
        let line = bar(t)
            .id("settings-status")
            .role(Role::Status)
            .aria_label(text.clone())
            .test_support()
            .items_center()
            .child(icon(IconName::Check, t.spacing.icon, t.colors.ok))
            .child(div().flex_1().min_w_0().truncate().child(text))
            .children(undo)
            .children(Kbd::new("settings-status-undo-key", "ctrl-z").filter(|_| can_undo));
        Some(line.into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Change;
    use crate::tokens::Density;

    #[test]
    fn the_status_names_the_setting_and_its_new_value() {
        let values = Values::default().with(&Change::density(Density::Compact));
        let set = status_text(Status::Set(Field::Density), &values);
        assert_eq!(set, "Density set to Compact · saved");
        let reset = Status::of_change(Field::Density, &Value::Reset);
        assert_eq!(
            status_text(reset, &Values::default()),
            "Density reset to Normal · saved"
        );
        let cleared = Values::default().with(&Change::text(Field::SummonKey, ""));
        assert_eq!(shown_value(&cleared, Field::SummonKey), "None");
    }
}
