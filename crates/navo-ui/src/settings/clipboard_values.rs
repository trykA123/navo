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
}
