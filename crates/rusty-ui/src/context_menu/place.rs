use gpui_kit::{Bounds, Pixels, Point, Size, point, px};
use rusty_core::shell_menu::Row;

use super::state::{Level, LevelStatus};

pub(super) const ROOT_WIDTH: f32 = 316.;
pub(super) const SUBMENU_WIDTH: f32 = 300.;
pub(super) const ROW: f32 = 28.;
pub(super) const HEADER: f32 = 24.;
pub(super) const SEPARATOR: f32 = 9.;
pub(super) const PAD: f32 = 4.;
pub(super) const FIELD: f32 = 32.;
pub(super) const CONTEXT_LINE: f32 = 26.;
pub(super) const FOOTER: f32 = 34.;
pub(super) const BORDER: f32 = 1.;
const MARGIN: f32 = 8.;
const GAP: f32 = 2.;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Frame {
    pub bounds: Bounds<Pixels>,
    pub list_height: Pixels,
}

pub(super) fn row_height(row: &Row) -> f32 {
    match row {
        Row::Header(_) => HEADER,
        Row::Separator => SEPARATOR,
        Row::Item(_) | Row::More { .. } => ROW,
    }
}

pub(super) fn row_top(rows: &[Row], index: usize) -> f32 {
    rows.iter().take(index).map(row_height).sum()
}

pub(super) fn list_height(level: &Level) -> f32 {
    match level.status {
        LevelStatus::Ready if level.rows.is_empty() => ROW,
        LevelStatus::Ready => level.rows.iter().map(row_height).sum(),
        LevelStatus::Loading => ROW,
        LevelStatus::Failed => ROW * 2.,
    }
}

pub(super) fn root_chrome() -> f32 {
    (PAD + BORDER) * 2. + FIELD + CONTEXT_LINE + FOOTER
}

fn room(area: Bounds<Pixels>) -> Pixels {
    area.size.height - px(MARGIN * 2.)
}

fn clamp(value: Pixels, low: Pixels, high: Pixels) -> Pixels {
    value.min(high.max(low)).max(low)
}

pub fn place_root(
    anchor: Point<Pixels>,
    size: Size<Pixels>,
    area: Bounds<Pixels>,
) -> Point<Pixels> {
    let margin = px(MARGIN);
    let (left, right) = (area.left() + margin, area.right() - margin);
    let (top, bottom) = (area.top() + margin, area.bottom() - margin);
    let x = if anchor.x + size.width <= right {
        anchor.x
    } else {
        anchor.x - size.width
    };
    let y = if anchor.y + size.height <= bottom || anchor.y - size.height < top {
        anchor.y
    } else {
        anchor.y - size.height
    };
    point(
        clamp(x, left, right - size.width),
        clamp(y, top, bottom - size.height),
    )
}

pub fn place_submenu(
    parent: Bounds<Pixels>,
    row_top: Pixels,
    size: Size<Pixels>,
    area: Bounds<Pixels>,
) -> Point<Pixels> {
    let margin = px(MARGIN);
    let gap = px(GAP);
    let x = if parent.right() + gap + size.width <= area.right() - margin {
        parent.right() + gap
    } else {
        parent.left() - gap - size.width
    };
    let y = row_top - px(PAD + BORDER);
    point(
        clamp(x, area.left() + margin, area.right() - margin - size.width),
        clamp(y, area.top() + margin, area.bottom() - margin - size.height),
    )
}

pub(super) fn root_frame(level: &Level, anchor: Point<Pixels>, area: Bounds<Pixels>) -> Frame {
    let chrome = px(root_chrome());
    let list_height = px(list_height(level)).min(room(area) - chrome);
    let size = Size::new(px(ROOT_WIDTH), list_height + chrome);
    Frame {
        bounds: Bounds::new(place_root(anchor, size, area), size),
        list_height,
    }
}

pub(super) struct SubmenuSpot {
    pub parent: Bounds<Pixels>,
    pub list_top: Pixels,
    pub scroll: Pixels,
}

pub(super) fn submenu_frame(
    level: &Level,
    parent_rows: &[Row],
    spot: SubmenuSpot,
    area: Bounds<Pixels>,
) -> Frame {
    let chrome = px((PAD + BORDER) * 2.);
    let list_height = px(list_height(level)).min(room(area) - chrome);
    let size = Size::new(px(SUBMENU_WIDTH), list_height + chrome);
    let row = spot.list_top + px(row_top(parent_rows, level.parent_row)) + spot.scroll;
    Frame {
        bounds: Bounds::new(place_submenu(spot.parent, row, size, area), size),
        list_height,
    }
}
