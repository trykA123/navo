use gpui_kit::Keystroke;

use crate::commands::{CommandSpec, Keymap};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    Set {
        command: &'static str,
        context: Option<String>,
        keys: Vec<String>,
    },
    Reset {
        command: &'static str,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub spec: CommandSpec,
    pub keys: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub key: String,
    pub other: CommandSpec,
    pub context: String,
}

impl Conflict {
    #[must_use]
    pub fn message(&self) -> String {
        todo!()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recorded {
    Key(String),
    Unbind,
    Cancel,
}

#[must_use]
pub fn recorded(stroke: &Keystroke) -> Option<Recorded> {
    let _ = stroke;
    todo!()
}

#[must_use]
pub fn conflict(keymap: &Keymap, proposal: &Proposal) -> Option<Conflict> {
    let _ = (keymap, proposal);
    todo!()
}

#[must_use]
pub fn edits(keymap: &Keymap, proposal: &Proposal) -> Vec<Edit> {
    let _ = (keymap, proposal);
    todo!()
}
