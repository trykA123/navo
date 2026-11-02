use gpui_kit::{Pixels, px};

use super::Spacing;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Density {
    Compact,
    #[default]
    Normal,
    Comfortable,
}

impl Density {
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Compact => Self::Normal,
            Self::Normal => Self::Comfortable,
            Self::Comfortable => Self::Compact,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Compact => "Compact",
            Self::Normal => "Normal",
            Self::Comfortable => "Comfortable",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rows {
    pub height: Pixels,
    pub inset: Pixels,
}

pub(super) fn rows(density: Density, spacing: &Spacing) -> Rows {
    match density {
        Density::Compact => Rows {
            height: px(20.),
            inset: spacing.md,
        },
        Density::Normal => Rows {
            height: spacing.row_height,
            inset: spacing.lg,
        },
        Density::Comfortable => Rows {
            height: px(30.),
            inset: spacing.xl,
        },
    }
}
