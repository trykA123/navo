mod model;
mod page;
mod proposal;

#[cfg(test)]
mod tests;

use std::sync::Arc;

pub use model::{Line, Row, area_label, lines, rows};
pub use page::{ShortcutsEvent, ShortcutsPage};
pub use proposal::{Conflict, Edit, Proposal, Recorded, conflict, edits, recorded};

use crate::commands::Keymap;
use crate::prelude::*;

pub const CONTEXT: &str = "Shortcuts";

gpui_kit::actions!(shortcuts, [PreviousRow, NextRow, Record, Cancel, Reset]);

pub fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "shortcuts");
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    pub keymap: Keymap,
    pub warnings: Vec<String>,
}

pub type SaveKeymap = Arc<dyn Fn(&[Edit]) -> Result<Saved, String> + Send + Sync>;
