use super::Viewer;
use crate::prelude::*;

const TEXT_EDIT: [(&str, &str); 4] = [
    ("Ctrl+F", "Find"),
    ("Ctrl+S", "Save"),
    ("Alt+3", "Hex"),
    ("Esc", "Close"),
];

impl Viewer {
    pub(in crate::window_view) fn keys(&self, cx: &App) -> Vec<(&'static str, &'static str)> {
        match &self.paged {
            Some(paged) => paged.read(cx).keys().to_vec(),
            None if self.editor.is_some() => TEXT_EDIT.to_vec(),
            None => vec![("Esc", "Close")],
        }
    }
}
