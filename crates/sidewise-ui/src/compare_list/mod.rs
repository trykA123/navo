//! The two panes' rows in compare mode (D10): aligned rows with gaps, status marks and colours.

mod bar;
mod gutter;
mod gutter_cell;
mod list;
mod row;
pub(crate) mod tone;

use std::rc::Rc;

use gpui_kit::{App, Window};

pub use bar::{BarEvent, CompareBar};
pub use gutter::{GutterLabel, GutterText, gutter_label};
pub use gutter_cell::{gutter_cell, gutter_width};
pub use list::CompareList;

/// What the user did on a compare list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompareEvent {
    /// A click on the row with this path.
    Select(String),
    /// A double click on the row with this path.
    Open(String),
}

type EventHandler = Rc<dyn Fn(&CompareEvent, &mut Window, &mut App)>;
