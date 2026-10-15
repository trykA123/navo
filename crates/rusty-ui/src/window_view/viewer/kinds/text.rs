use super::{EDIT_LIMIT, Shape, Slot, Sniff, ViewerKind};
use crate::window_view::viewer::PagedKind;

const TEXT: u8 = 50;
const BINARY: u8 = 1;

pub(super) struct Text;

impl ViewerKind for Text {
    fn id(&self) -> &'static str {
        "text"
    }

    fn name(&self) -> &'static str {
        "Text"
    }

    fn slot(&self) -> Slot {
        Slot::Text
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if sniff.looks_binary() { BINARY } else { TEXT }
    }

    fn shape(&self, sniff: &Sniff<'_>) -> Shape {
        if sniff.looks_binary() || sniff.len > EDIT_LIMIT {
            Shape::Paged(PagedKind::Text)
        } else {
            Shape::Editor
        }
    }
}
