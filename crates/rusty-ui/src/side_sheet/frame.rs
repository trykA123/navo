use gpui_kit::{Pixels, px};

const SHARE: f32 = 0.42;
const MIN_WIDTH: f32 = 320.;
const ACTIVE_KEEPS: f32 = 360.;
const NARROW_BODY: f32 = 1100.;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Side {
    #[default]
    Left,
    Right,
}

impl Side {
    #[must_use]
    pub fn other(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SheetFrame {
    pub body: Pixels,
    pub passive_card: Pixels,
    pub active: Side,
}

impl SheetFrame {
    pub fn new(body: Pixels, passive_card: Pixels, active: Side) -> Self {
        Self {
            body,
            passive_card,
            active,
        }
    }

    pub fn side(&self) -> Side {
        self.active.other()
    }

    pub fn is_narrow(&self) -> bool {
        self.body <= px(NARROW_BODY)
    }

    pub fn base_width(&self) -> Pixels {
        if self.is_narrow() {
            self.passive_card
        } else {
            (self.body * SHARE).round()
        }
    }

    pub fn clamp(&self, width: Pixels) -> Pixels {
        width.min(self.body - px(ACTIVE_KEEPS)).max(px(MIN_WIDTH))
    }

    pub fn width(&self, delta: Pixels) -> Pixels {
        self.clamp(self.base_width() + delta)
    }

    pub(super) fn delta_for(&self, width: Pixels) -> Pixels {
        self.clamp(width) - self.base_width()
    }
}
