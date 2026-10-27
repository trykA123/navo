use super::FerriteWindow;
use super::font_settings::ui_scale;
use super::keys::ResetColumnWidths;
use crate::file_list::{ListParts, SortBy, UserWidths};
use crate::prelude::*;
use crate::settings::{Change, Field, Value, Values, WindowParts};

const WIDTH_FIELDS: [Field; 3] = [
    Field::WindowExtWidth,
    Field::WindowSizeWidth,
    Field::WindowDateWidth,
];

pub(super) fn list_parts(parts: WindowParts) -> ListParts {
    ListParts {
        ext: parts.ext_column,
        size: parts.size_column,
        date: parts.date_column,
        icons: parts.icons,
    }
}

fn width_field(by: SortBy) -> Option<Field> {
    match by {
        SortBy::Ext => Some(Field::WindowExtWidth),
        SortBy::Size => Some(Field::WindowSizeWidth),
        SortBy::Date => Some(Field::WindowDateWidth),
        SortBy::Name => None,
    }
}

fn factor(values: &Values) -> f32 {
    f32::from(ui_scale(values)) / 100.
}

pub(super) fn user_widths(values: &Values) -> UserWidths {
    let scale = factor(values);
    let width = |field: Field| match values.get(field) {
        Value::Number(stored) if *stored > 0 => Some(px(*stored as f32 * scale)),
        _ => None,
    };
    UserWidths {
        ext: width(Field::WindowExtWidth),
        size: width(Field::WindowSizeWidth),
        date: width(Field::WindowDateWidth),
    }
}

pub(super) fn column_keys(root: Div, cx: &mut Context<FerriteWindow>) -> Div {
    root.on_action(cx.listener(|this, _: &ResetColumnWidths, window, cx| {
        this.reset_column_widths(window, cx);
    }))
}

impl FerriteWindow {
    pub(super) fn resize_column(
        &mut self,
        (by, width): (SortBy, Pixels),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(field) = width_field(by) else {
            return;
        };
        let stored = if width > px(0.) {
            (f32::from(width) / factor(&self.values)).ceil().max(1.) as i64
        } else {
            0
        };
        let change = Change {
            field,
            value: Value::Number(stored),
        };
        self.change_setting(change, window, cx);
    }

    fn reset_column_widths(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for field in WIDTH_FIELDS {
            if self.values.is_changed(field) {
                self.change_setting(Change::reset(field), window, cx);
            }
        }
    }
}
