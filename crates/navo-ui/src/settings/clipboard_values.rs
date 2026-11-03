use crate::clip_palette::PopUpPosition;

use super::fields::Field;
use super::values::{Value, Values};

impl Values {
    pub fn clipboard_on(&self) -> bool {
        self.flag(Field::ClipboardOn)
    }

    pub fn records_clipboard(&self) -> bool {
        self.flag(Field::ClipboardRecord)
    }

    pub fn clipboard_key(&self) -> &str {
        match self.get(Field::ClipboardKey) {
            Value::Text(text) => text,
            _ => "",
        }
    }

    pub fn popup_position(&self) -> PopUpPosition {
        match self.get(Field::ClipboardPosition) {
            Value::Pick(1) => PopUpPosition::Cursor,
            Value::Pick(2) => PopUpPosition::Center,
            _ => PopUpPosition::Last,
        }
    }
}
