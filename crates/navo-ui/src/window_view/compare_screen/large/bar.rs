//! The large body's place in the file (D16) and its scrollbar. The list holds a window of at
//! most 262,144 rows from `base`, so `f32` pixel offsets stay exact at 10M lines. The scrollbar
//! handle maps the thumb to the absolute top row over the whole file and moves the window to it.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::base::ScrollbarHandle;
use gpui_kit::{Bounds, Pixels, Point, Size, point, px, size};

use crate::primitives::ListScroll;

/// Rows the list holds at once; their offsets (under 2^24 px at 64 px rows) are exact in `f32`.
const WINDOW: u64 = 1 << 18;
/// Rows assumed in view before the first layout measured the list.
const FIRST_VIEW: u64 = 100;
/// A row fraction that absorbs `f32` rounding when an offset turns back into rows.
const SNAP: f32 = 1. / 32.;

/// The window start, the file's row count, the row height and whether the thumb is held.
#[derive(Clone, Copy, Default)]
struct Place {
    base: u64,
    count: u64,
    /// The count the thumb maps over while it is held, so hunks arriving mid-drag do not
    /// change what a still pointer means.
    held: Option<u64>,
    row_height: f32,
    dragging: bool,
}

impl Place {
    /// The row count the scrollbar sees: frozen at the grab, else the file's.
    fn seen(&self) -> u64 {
        self.held.unwrap_or(self.count)
    }
}

/// The shared place of the large body and the scrollbar handle over the whole file.
#[derive(Clone)]
pub(super) struct FileBar {
    place: Rc<Cell<Place>>,
    list: ListScroll,
}

impl FileBar {
    /// A bar over `list` for rows `row_height` tall.
    pub(super) fn new(list: &ListScroll, row_height: Pixels) -> Self {
        let place = Place {
            row_height: f32::from(row_height),
            ..Place::default()
        };
        Self {
            place: Rc::new(Cell::new(place)),
            list: list.clone(),
        }
    }

    /// Edits the shared place.
    fn edit(&self, change: impl FnOnce(&mut Place)) {
        let mut place = self.place.get();
        change(&mut place);
        self.place.set(place);
    }

    /// Sets the row height in pixels.
    pub(super) fn set_row_height(&self, row_height: Pixels) {
        self.edit(|place| place.row_height = f32::from(row_height));
    }

    /// Sets the file's row count.
    pub(super) fn set_count(&self, count: u64) {
        self.edit(|place| place.count = count);
    }

    /// The first row of the list's window.
    pub(super) fn base(&self) -> u64 {
        self.place.get().base
    }

    /// The rows the list holds, from `base`.
    pub(super) fn window_len(&self) -> u64 {
        let place = self.place.get();
        place.count.saturating_sub(place.base).min(WINDOW)
    }

    /// Pixels scrolled inside the list's window, never negative.
    fn scrolled(&self) -> f32 {
        (-f32::from(self.list.offset_y())).max(0.)
    }

    /// Whole rows in `pixels`; zero before the row height is known.
    fn rows_in(&self, pixels: f32) -> u64 {
        let height = self.place.get().row_height;
        let rows = (pixels / height + SNAP).floor();
        if height > 0. && rows.is_finite() && rows > 0. {
            rows as u64
        } else {
            0
        }
    }

    /// The top row: the window's first row plus the whole rows scrolled inside it.
    pub(super) fn top(&self) -> u64 {
        self.base().saturating_add(self.rows_in(self.scrolled()))
    }

    /// Pixels scrolled into the top row.
    pub(super) fn sub(&self) -> f32 {
        let whole = self.rows_in(self.scrolled()) as f32 * self.place.get().row_height;
        (self.scrolled() - whole).max(0.)
    }

    /// Rows the list showed in the last layout, or 100 before the first one.
    pub(super) fn visible(&self) -> u64 {
        match self.rows_in(f32::from(self.list.viewport_height())) {
            0 => FIRST_VIEW,
            rows => rows,
        }
    }

    /// Puts row `row` on top with `sub` pixels scrolled into it. The window moves to centre the
    /// row when it nears an edge the file goes past, never while the thumb is held unless the
    /// row is outside the window.
    pub(super) fn put(&self, row: u64, sub: f32) {
        let Place {
            base,
            count,
            row_height,
            dragging,
            ..
        } = self.place.get();
        let row = row.min(count.saturating_sub(1));
        let far = row.checked_sub(base).is_none_or(|inside| {
            let margin = (inside < WINDOW / 4 && base > 0)
                || (inside >= WINDOW / 4 * 3 && base.saturating_add(WINDOW) < count);
            inside >= WINDOW || (margin && !dragging)
        });
        let base = if far || base >= count.max(1) {
            row.saturating_sub(WINDOW / 2)
                .min(count.saturating_sub(WINDOW))
        } else {
            base
        };
        self.edit(|place| place.base = base);
        let inside = row.saturating_sub(base) as f32;
        let sub = sub.clamp(0., row_height);
        self.list.set_offset_y(px(-(inside * row_height + sub)));
    }
}

impl ScrollbarHandle for FileBar {
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.list.bounds()
    }

    fn offset(&self) -> Point<Pixels> {
        let row_height = f64::from(self.place.get().row_height);
        let above = self.base() as f64 * row_height + f64::from(self.scrolled());
        point(px(0.), px(-above as f32))
    }

    /// Moves the top row to where the thumb says, over the whole file.
    fn set_offset(&self, offset: Point<Pixels>) {
        let place = self.place.get();
        let row_height = f64::from(place.row_height);
        if row_height <= 0. {
            return;
        }
        let rows = (-f64::from(f32::from(offset.y)) / row_height).max(0.);
        let row = (rows.floor() as u64).min(place.seen().saturating_sub(1));
        self.put(row, ((rows - rows.floor()) * row_height) as f32);
    }

    fn content_size(&self) -> Size<Pixels> {
        let place = self.place.get();
        let height = place.seen() as f64 * f64::from(place.row_height);
        size(self.list.bounds().size.width, px(height as f32))
    }

    fn start_drag(&self) {
        self.edit(|place| {
            place.dragging = true;
            place.held = Some(place.count);
        });
    }

    fn end_drag(&self) {
        self.edit(|place| {
            place.dragging = false;
            place.held = None;
        });
    }
}
