use super::KeymapFileError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeymapEdit {
    Set {
        command: String,
        context: Option<String>,
        keys: Vec<String>,
    },
    Reset {
        command: String,
    },
}

pub fn patch_keymap(text: &str, edits: &[KeymapEdit]) -> Result<String, KeymapFileError> {
    let _ = (text, edits);
    todo!()
}
