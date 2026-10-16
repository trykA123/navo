use rusty_ui::prelude::*;
use rusty_ui::settings::{self, Change, FIELDS, Field, SettingsBody, Values};
use rusty_ui::side_sheet::{SheetFrame, SheetState, SideSheet};
use rusty_ui::tokens::{Mode, Theme, ThemeChoice};

pub fn sheet(t: &Theme, state: &Entity<SheetState>, frame: SheetFrame, field: Field) -> SideSheet {
    let theme = match t.mode {
        Mode::Light => ThemeChoice::Light,
        Mode::Dark => ThemeChoice::Dark,
    };
    let values = Values::default()
        .with(&Change::theme(theme))
        .with(&Change::text(Field::EditorPath, r"C:\Tools\code.exe"));
    let row = FIELDS
        .iter()
        .position(|spec| spec.field == field)
        .unwrap_or_default();
    settings::sheet(state, frame, SettingsBody::new(values, row)).close("escape", |_, _| {})
}
