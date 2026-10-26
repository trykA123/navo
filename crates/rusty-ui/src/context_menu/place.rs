use gpui_kit::{Bounds, Pixels, Point, Size, point, px};
use rusty_core::shell_menu::{Row, ShellMenu};

use super::state::PageStatus;

pub(super) const MIN_WIDTH: f32 = 316.;
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
    pub has_back: bool,
}

pub(super) fn row_height(row: &Row) -> f32 {
    match row {
        Row::Back { .. } => 0.,
        Row::Separator => SEPARATOR,
        Row::Item(_) => ROW,
    }
}

pub(super) fn list_height(rows: &[Row], status: PageStatus) -> f32 {
    let listed = rows.iter().filter(|row| !matches!(row, Row::Back { .. }));
    match status {
        PageStatus::Ready if listed.clone().next().is_none() => ROW,
        PageStatus::Ready => listed.map(row_height).sum(),
        PageStatus::Loading => ROW,
        PageStatus::Failed => ROW * 2.,
    }
}

pub(super) fn chrome(has_back: bool) -> f32 {
    let header = if has_back {
        CONTEXT_LINE + ROW
    } else {
        CONTEXT_LINE
    };
    (PAD + BORDER) * 2. + FIELD + header + FOOTER
}

/// The width a page asks for, from its text lengths; the panel keeps the widest one seen.
pub(super) fn page_width(rows: &[Row], menu: &ShellMenu) -> f32 {
    let row_width = |row: &Row| match row {
        Row::Back { title } => ROW_CHROME + chars(title) * TEXT_CHAR,
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
    (widest + (PAD + BORDER) * 2.).max(MIN_WIDTH)
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
    let chrome = px(chrome(panel.has_back));
    let list = px(panel.list).min(area.size.height - px(MARGIN * 2.) - chrome);
    let width = panel.width.min(area.size.width - px(MARGIN * 2.));
    place_root(anchor, Size::new(width, list + chrome), area)
}

/// Keeps the top-left corner; a wider page moves left and a taller one scrolls, never leaving `area`.
pub(super) fn panel_frame(panel: Panel, area: Bounds<Pixels>) -> Frame {
    let margin = px(MARGIN);
    let chrome = px(chrome(panel.has_back));
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
