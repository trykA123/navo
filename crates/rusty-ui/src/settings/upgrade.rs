use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use super::fields::{Control, FIELDS, Field};
use super::values::{Change, Value, Values};
use super::{Handler, SheetEvent};
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconName, icon};
use crate::tokens::Theme;

const LIST_MAX: f32 = 168.;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultShift {
    pub key: String,
    pub old: String,
    pub new: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moved {
    pub shift: DefaultShift,
    pub field: Field,
    keep: Option<Value>,
    new: Option<Value>,
}

fn parsed(canonical: &str) -> Option<toml::Value> {
    let mut table: toml::Table = toml::from_str(&format!("v = {canonical}")).ok()?;
    table.remove("v")
}

fn plain(canonical: &str) -> String {
    match parsed(canonical) {
        Some(toml::Value::String(text)) => text,
        Some(value) => value.to_string(),
        None => canonical.to_owned(),
    }
}

fn value_of(control: Control, value: toml::Value) -> Option<Value> {
    match (control, value) {
        (Control::Choices(options, _), toml::Value::String(name)) => options
            .iter()
            .position(|option| option.toml == name)
            .map(Value::Pick),
        (Control::Toggle, toml::Value::Boolean(flag)) => Some(Value::Flag(flag)),
        (Control::Number, toml::Value::Integer(number)) => Some(Value::Number(number)),
        (Control::Text, toml::Value::String(text)) => Some(Value::Text(text.into())),
        _ => None,
    }
}

impl Moved {
    #[must_use]
    pub fn of(shift: &DefaultShift) -> Option<Self> {
        let spec = FIELDS.iter().find(|spec| spec.key == shift.key)?;
        Some(Self {
            shift: shift.clone(),
            field: spec.field,
            keep: parsed(&shift.old).and_then(|old| value_of(spec.control, old)),
            new: parsed(&shift.new).and_then(|new| value_of(spec.control, new)),
        })
    }

    #[must_use]
    pub fn line(&self) -> String {
        format!(
            "{}: {} → {}",
            self.shift.key,
            plain(&self.shift.old),
            plain(&self.shift.new)
        )
    }

    #[must_use]
    pub fn is_current(&self, values: &Values) -> bool {
        self.new.as_ref() == Some(values.get(self.field))
    }

    #[must_use]
    pub fn keep_change(&self) -> Option<Change> {
        self.keep.clone().map(|value| Change {
            field: self.field,
            value,
        })
    }
}

fn button_on(button: Button, send: Option<&Handler>, event: SheetEvent) -> Button {
    match send.cloned() {
        Some(send) => button.on_click(move |_, window, cx| send(&event, window, cx)),
        None => button,
    }
}

fn item(t: &Theme, index: usize, moved: &Moved, send: Option<&Handler>) -> AnyElement {
    let spec = moved.field.spec();
    let keep = moved.keep_change().is_some().then(|| {
        let button = Button::new(("settings-keep", index), "Keep old value")
            .variant(ButtonVariant::Secondary)
            .tooltip("Write the old value to navo.toml as your own");
        button_on(button, send, SheetEvent::Keep(moved.field))
    });
    div()
        .id(("settings-upgrade-item", index))
        .role(Role::ListItem)
        .aria_label(format!("{}, {}", spec.label(), moved.line()))
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.xl)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(div().font_weight(FontWeight::MEDIUM).child(spec.label()))
                .child(
                    div()
                        .font_family(t.typography.mono_font.clone())
                        .text_size(t.typography.small)
                        .text_color(t.colors.muted)
                        .child(moved.line()),
                ),
        )
        .children(keep)
        .into_any_element()
}

fn head(t: &Theme, count: usize, send: Option<&Handler>) -> Div {
    let title = if count == 1 {
        "A default changed in this version".to_owned()
    } else {
        format!("{count} defaults changed in this version")
    };
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .child(icon(IconName::Info, t.spacing.icon, t.colors.accent_text))
        .child(
            div()
                .flex_1()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(button_on(
            Button::new("settings-dismiss", "Dismiss").variant(ButtonVariant::Ghost),
            send,
            SheetEvent::DismissUpgrade,
        ))
}

pub(super) fn card(t: &Theme, items: &[Moved], send: Option<&Handler>) -> AnyElement {
    let list = div()
        .id("settings-upgrade-list")
        .flex()
        .flex_col()
        .gap(t.spacing.md)
        .max_h(px(LIST_MAX))
        .overflow_y_scroll()
        .children(
            items
                .iter()
                .enumerate()
                .map(|(index, moved)| item(t, index, moved, send)),
        );
    div()
        .id("settings-upgrade")
        .role(Role::Note)
        .aria_label("Defaults changed")
        .test_support()
        .flex()
        .flex_col()
        .flex_none()
        .gap(t.spacing.md)
        .mx(t.spacing.xl)
        .mt(t.spacing.md)
        .p(t.spacing.lg)
        .rounded(t.radii.md)
        .border_1()
        .border_color(t.surfaces.card_edge)
        .bg(t.surfaces.field)
        .text_size(t.typography.small)
        .child(head(t, items.len(), send))
        .child(
            div().text_color(t.colors.muted).child(
                "These settings follow Navo's defaults. Keep an old value to make it yours.",
            ),
        )
        .child(list)
        .into_any_element()
}
