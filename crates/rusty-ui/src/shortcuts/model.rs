use crate::commands::{CommandSpec, Keymap};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub spec: CommandSpec,
    pub keys: Vec<String>,
    pub is_changed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    Area(&'static str),
    Row(usize),
}

#[must_use]
pub fn rows(keymap: &Keymap) -> Vec<Row> {
    let _ = keymap;
    todo!()
}

#[must_use]
pub fn lines(rows: &[Row], query: &str) -> Vec<Line> {
    let _ = (rows, query);
    todo!()
}

#[must_use]
pub fn area_label(area: &str) -> String {
    let _ = area;
    todo!()
}
