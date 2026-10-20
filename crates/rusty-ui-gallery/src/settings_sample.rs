use rusty_ui::prelude::*;
use rusty_ui::primitives::TextFieldState;
use rusty_ui::settings::{
    self, Change, DefaultShift, FIELDS, Field, Filter, Moved, SettingsBody, Values, find_field,
};
use rusty_ui::side_sheet::{SheetFrame, SheetState, SideSheet};
use rusty_ui::tokens::{Mode, Theme, ThemeChoice};

#[derive(Clone, Default)]
pub struct Extras {
    pub has_notice: bool,
    pub find: Option<(Entity<TextFieldState>, &'static str)>,
}

pub struct Shown<'a> {
    pub field: Field,
    pub extras: &'a Extras,
}

fn notice() -> Vec<Moved> {
    [
        ("files.sort", "\"name\"", "\"date\""),
        ("files.folders-first", "false", "true"),
    ]
    .iter()
    .filter_map(|(key, old, new)| {
        Moved::of(&DefaultShift {
            key: (*key).to_owned(),
            old: (*old).to_owned(),
            new: (*new).to_owned(),
        })
    })
    .collect()
}

pub fn sheet(t: &Theme, state: &Entity<SheetState>, frame: SheetFrame, shown: &Shown) -> SideSheet {
    let theme = match t.mode {
        Mode::Light => ThemeChoice::Light,
        Mode::Dark => ThemeChoice::Dark,
    };
    let values = Values::default()
        .with(&Change::theme(theme))
        .with(&Change::text(Field::EditorPath, r"C:\Tools\code.exe"));
    let row = FIELDS
        .iter()
        .position(|spec| spec.field == shown.field)
        .unwrap_or_default();
    let body = SettingsBody::new(values, row);
    let body = match &shown.extras.find {
        Some((input, query)) => body
            .filter(Filter::new(query))
            .find(Some(find_field(t, input).into_any_element())),
        None => body,
    };
    let body = body.upgrade(if shown.extras.has_notice {
        notice()
    } else {
        Vec::new()
    });
    settings::sheet(state, frame, body).close("escape", |_, _| {})
}
