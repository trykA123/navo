mod combo;
mod drawer;
mod groups;
mod hold;
mod layer;

#[cfg(test)]
mod tests;

use gpui_kit::Pixels;

pub use combo::Combo;
pub use drawer::{HintsDrawer, TurnOff, drawer_height};
pub use hold::{Arm, Delay, Hold};
pub use layer::{Entry, Group, Layer, key_cap, layer_in};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum HintsMode {
    #[default]
    All,
    Fkeys,
    Off,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Cover {
    pub top: Pixels,
    pub gap: Pixels,
}

#[must_use]
pub fn rows_to_lift(cursor_bottom: Pixels, cover: Cover, row_height: Pixels) -> usize {
    let overlap = (cursor_bottom + cover.gap - cover.top) / row_height;
    if overlap.is_finite() && overlap > 0. {
        overlap.ceil() as usize
    } else {
        0
    }
}
