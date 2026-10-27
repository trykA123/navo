use gpui_kit::{Bounds, Pixels, Point, Size, point, px};
use rusty_core::shell_menu::{Row, ShellMenu};

use super::state::PageStatus;

pub(super) const MIN_WIDTH: f32 = 316.;
pub(super) const FLYOUT_MIN_WIDTH: f32 = 200.;
pub(super) const ROW: f32 = 28.;
pub(super) const SEPARATOR: f32 = 9.;
pub(super) const PAD: f32 = 4.;
pub(super) const FIELD: f32 = 32.;
pub(super) const CONTEXT_LINE: f32 = 26.;
pub(super) const FOOTER: f32 = 34.;
pub(super) const BORDER: f32 = 1.;
const MARGIN: f32 = 8.;
const TEXT_CHAR: f32 = 7.;
const KEY_CHAR: f32 = 6.5;
const ROW_CHROME: f32 = 48.;
const KEY_GAP: f32 = 12.;
const CHEVRON: f32 = 28.;
const OVERLAP: f32 = 2.;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Frame {
    pub bounds: Bounds<Pixels>,
    pub list_height: Pixels,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Panel {
    pub origin: Point<Pixels>,
    pub width: Pixels,
    pub list: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    Right,
    Left,
}

/// A flyout beside its parent panel, its first row level with the row that opened it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Flyout {
    pub parent: Bounds<Pixels>,
    pub row_top: Pixels,
    pub width: Pixels,
    pub list: f32,
    pub side: Side,
}

pub(super) fn row_height(row: &Row) -> f32 {
    match row {
        Row::Separator => SEPARATOR,
        Row::Item(_) => ROW,
    }
}

pub(super) fn rows_height(rows: &[Row]) -> f32 {
    rows.iter().map(row_height).sum()
}

pub(super) fn list_height(rows: &[Row], status: PageStatus) -> f32 {
    match status {
        PageStatus::Ready if rows.is_empty() => ROW,
        PageStatus::Ready => rows_height(rows),
        PageStatus::Loading => ROW,
        PageStatus::Failed => ROW * 2.,
    }
}

pub(super) fn chrome() -> f32 {
    (PAD + BORDER) * 2. + FIELD + CONTEXT_LINE + FOOTER
}

pub(super) fn flyout_chrome() -> f32 {
    (PAD + BORDER) * 2.
}

/// Where the list starts below the top edge of a panel.
pub(super) fn list_top(level: usize) -> f32 {
    if level == 0 {
        BORDER + PAD + FIELD + CONTEXT_LINE
    } else {
        BORDER + PAD
    }
}

/// The width a page asks for, from its text lengths; the root keeps the widest one seen.
pub(super) fn page_width(rows: &[Row], menu: &ShellMenu) -> f32 {
    let row_width = |row: &Row| match row {
        Row::Separator => 0.,
        Row::Item(item) => {
            let label = (chars(&item.text)
                + item.crumb.iter().map(|part| chars(part) + 3.).sum::<f32>())
                * TEXT_CHAR;
            let keys = menu
                .item(&item.path)
                .and_then(|source| source.accelerator.as_deref())
                .filter(|_| !item.opens_submenu)
                .map_or(0., |keys| KEY_GAP + chars(keys) * KEY_CHAR);
            let chevron = if item.opens_submenu { CHEVRON } else { 0. };
            ROW_CHROME + label + keys + chevron
        }
    };
    let widest = rows.iter().map(row_width).fold(0., f32::max);
    widest + (PAD + BORDER) * 2.
}

fn chars(text: &str) -> f32 {
    text.chars().count() as f32
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

pub(super) fn opening_origin(
    panel: Panel,
    anchor: Point<Pixels>,
    area: Bounds<Pixels>,
) -> Point<Pixels> {
    let chrome = px(chrome());
    let list = px(panel.list).min(area.size.height - px(MARGIN * 2.) - chrome);
    let width = panel.width.min(area.size.width - px(MARGIN * 2.));
    place_root(anchor, Size::new(width, list + chrome), area)
}

/// Keeps the top-left corner; a wider page moves left and a taller one scrolls, never leaving `area`.
pub(super) fn panel_frame(panel: Panel, area: Bounds<Pixels>) -> Frame {
    let margin = px(MARGIN);
    let chrome = px(chrome());
    let width = panel.width.min(area.size.width - margin * 2.);
    let x = clamp(
        panel.origin.x,
        area.left() + margin,
        area.right() - margin - width,
    );
    let below = area.bottom() - margin - panel.origin.y - chrome;
    let list_height = px(panel.list).min(below.max(px(ROW)));
    let height = list_height + chrome;
    let y = clamp(
        panel.origin.y,
        area.top() + margin,
        area.bottom() - margin - height,
    );
    Frame {
        bounds: Bounds::new(point(x, y), Size::new(width, height)),
        list_height,
    }
}

/// Opens on `side` when it fits, else on the other side when that fits; then shifts to stay in `area`.
pub(super) fn flyout_frame(flyout: Flyout, area: Bounds<Pixels>) -> (Frame, Side) {
    let margin = px(MARGIN);
    let chrome = px(flyout_chrome());
    let width = flyout.width.min(area.size.width - margin * 2.);
    let room = (area.size.height - margin * 2. - chrome).max(px(ROW));
    let list_height = px(flyout.list).min(room);
    let height = list_height + chrome;
    let right = flyout.parent.right() - px(OVERLAP);
    let left = flyout.parent.left() - width + px(OVERLAP);
    let fits_right = right + width <= area.right() - margin;
    let fits_left = left >= area.left() + margin;
    let side = match flyout.side {
        Side::Right if !fits_right && fits_left => Side::Left,
        Side::Left if !fits_left && fits_right => Side::Right,
        side => side,
    };
    let x = match side {
        Side::Right => right,
        Side::Left => left,
    };
    let x = clamp(x, area.left() + margin, area.right() - margin - width);
    let y = clamp(
        flyout.row_top - px(PAD + BORDER),
        area.top() + margin,
        area.bottom() - margin - height,
    );
    let frame = Frame {
        bounds: Bounds::new(point(x, y), Size::new(width, height)),
        list_height,
    };
    (frame, side)
}
