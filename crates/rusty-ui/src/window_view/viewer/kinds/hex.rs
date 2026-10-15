use super::{Shape, Slot, Sniff, ViewerKind};
use crate::window_view::viewer::PagedKind;

const BINARY: u8 = 60;
const TEXT: u8 = 10;

pub(super) struct Hex;

impl ViewerKind for Hex {
    fn id(&self) -> &'static str {
        "hex"
    }

    fn name(&self) -> &'static str {
        "Hex"
    }

    fn slot(&self) -> Slot {
        Slot::Hex
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if sniff.looks_binary() { BINARY } else { TEXT }
    }

    fn shape(&self, _: &Sniff<'_>) -> Shape {
        Shape::Paged(PagedKind::Hex)
    }
}
