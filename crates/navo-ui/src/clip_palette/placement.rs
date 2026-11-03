//! Where and how big the clipboard pop-up opens: the default size, the remembered size and
//! position, and the three placements. Pure geometry in logical pixels.

use gpui_kit::DisplayId;
use serde::{Deserialize, Serialize};

pub(super) const DEFAULT_SIZE: Size = Size {
    width: 960.,
    height: 600.,
};
pub(super) const MIN_SIZE: Size = Size {
    width: 560.,
    height: 320.,
};
/// The most of a display's visible area the pop-up may take, per axis.
const MAX_SHARE: f32 = 0.9;

/// Where the pop-up opens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PopUpPosition {
    /// Where it was last closed, when that display still exists.
    #[default]
    Last,
    Cursor,
    Center,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Size {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PopUpRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

pub(super) use PopUpRect as Rect;

impl Rect {
    fn size(self) -> Size {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    /// Whether both rectangles differ by under one pixel on every side.
    pub(super) fn same_as(self, other: Self) -> bool {
        (self.x - other.x).abs() < 1.
            && (self.y - other.y).abs() < 1.
            && (self.width - other.width).abs() < 1.
            && (self.height - other.height).abs() < 1.
    }
}

/// Where the pop-up was when it last closed.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PopUpPlacement {
    pub display: Option<[u8; 16]>,
    pub rect: Rect,
}

pub(super) use PopUpPlacement as Remembered;

pub(super) fn remembered_display(
    last: Option<Remembered>,
    displays: &[(DisplayId, [u8; 16])],
) -> Option<DisplayId> {
    let uuid = last?.display?;
    displays
        .iter()
        .find(|(_, found)| *found == uuid)
        .map(|(id, _)| *id)
}

/// `wanted` kept within 90 % of the display's visible area, and at least the minimum size
/// unless the display is smaller than that.
pub(super) fn fit(wanted: Size, visible: Rect) -> Size {
    let axis = |wanted: f32, room: f32, min: f32| {
        wanted.min(room * MAX_SHARE).max(min.min(room * MAX_SHARE))
    };
    Size {
        width: axis(wanted.width, visible.width, MIN_SIZE.width),
        height: axis(wanted.height, visible.height, MIN_SIZE.height),
    }
}

/// The size to open with: the remembered one, else the default, fitted to the display.
pub(super) fn open_size(remembered: Option<Remembered>, visible: Rect) -> Size {
    fit(
        remembered.map_or(DEFAULT_SIZE, |last| last.rect.size()),
        visible,
    )
}

fn inside(x: f32, y: f32, size: Size, visible: Rect) -> (f32, f32) {
    let clamp = |at: f32, from: f32, room: f32, extent: f32| at.min(from + room - extent).max(from);
    (
        clamp(x, visible.x, visible.width, size.width),
        clamp(y, visible.y, visible.height, size.height),
    )
}

/// The mouse position and the remembered origin, either of which may be unknown.
pub(super) type Spots = (Option<(f32, f32)>, Option<(f32, f32)>);

/// The rectangle for a new pop-up. `last` is the remembered origin, already known to be on
/// this display; `cursor` is the mouse position.
pub(super) fn place(
    position: PopUpPosition,
    size: Size,
    visible: Rect,
    (cursor, last): Spots,
) -> Rect {
    let centred = (
        visible.x + (visible.width - size.width) / 2.,
        visible.y + (visible.height - size.height) / 2.,
    );
    let wanted = match position {
        PopUpPosition::Center => centred,
        PopUpPosition::Cursor => cursor.map_or(centred, |(x, y)| {
            (x - size.width / 2., y - size.height / 2.)
        }),
        PopUpPosition::Last => last.unwrap_or(centred),
    };
    let (x, y) = inside(wanted.0, wanted.1, size, visible);
    Rect {
        x,
        y,
        width: size.width,
        height: size.height,
    }
}

#[cfg(test)]
#[path = "placement_tests.rs"]
mod tests;
