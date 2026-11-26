//! Pure density metrics and overflow-fitting helpers for the button bar.

#![cfg_attr(
    not(test),
    expect(dead_code, reason = "BB19 seam; wired by the button bar view in BB22")
)]

use crate::Pixels;
use crate::tokens::{Density, Spacing};

/// Pixel dimensions used to lay out the button bar at a given density.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BarMetrics {
    /// Total height of the button bar strip.
    pub height: Pixels,
    /// Square size of an icon-only button.
    pub button: Pixels,
    /// Icon glyph size drawn inside a button.
    pub icon: Pixels,
    /// Width of a separator slot.
    pub separator: Pixels,
    /// Horizontal gap between adjacent slots.
    pub gap: Pixels,
}

/// Computes button bar pixel dimensions for `spacing` and `density`.
#[must_use]
pub(crate) fn metrics(spacing: &Spacing, density: Density) -> BarMetrics {
    let height = match density {
        Density::Compact => spacing.button_bar_height - spacing.xs,
        Density::Normal => spacing.button_bar_height,
        Density::Comfortable => spacing.button_bar_height + spacing.xs,
    };
    BarMetrics {
        height,
        button: spacing.button_size,
        icon: spacing.button_icon,
        separator: spacing.button_separator,
        gap: spacing.xxs,
    }
}

/// One top-level layout slot in the button bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Slot {
    /// Button slot with its measured width in pixels.
    Button(f32),
    /// Visual separator between groups of buttons.
    Separator,
}

fn slot_width(slot: Slot, separator: f32) -> f32 {
    match slot {
        Slot::Button(width) => width.max(0.0),
        Slot::Separator => separator.max(0.0),
    }
}

fn total_width(slots: &[Slot], separator: f32, gap: f32) -> f32 {
    let mut total = 0.0_f32;
    for (index, &slot) in slots.iter().enumerate() {
        if index > 0 {
            total += gap;
        }
        total += slot_width(slot, separator);
    }
    total
}

/// Returns how many leading slots fit in `room`, reserving `more_width` on overflow.
///
/// Any trailing `Slot::Separator` immediately before the overflow cut is dropped.
#[must_use]
pub(crate) fn fit(slots: &[Slot], room: f32, more_width: f32, separator: f32, gap: f32) -> usize {
    if slots.is_empty() || room <= 0.0 || room.is_nan() {
        return 0;
    }

    let gap = gap.max(0.0);
    let more_width = more_width.max(0.0);

    if total_width(slots, separator, gap) <= room {
        return slots.len();
    }

    let mut used = 0.0_f32;
    let mut shown = 0;
    for &slot in slots {
        let step = if shown == 0 {
            slot_width(slot, separator)
        } else {
            gap + slot_width(slot, separator)
        };
        if used + step + gap + more_width <= room {
            used += step;
            shown += 1;
        } else {
            break;
        }
    }

    while shown > 0 && matches!(slots.get(shown - 1), Some(Slot::Separator)) {
        shown -= 1;
    }

    shown
}

#[cfg(test)]
#[path = "fit_tests.rs"]
mod tests;
