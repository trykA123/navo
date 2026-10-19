use gpui_kit::{App, Bounds, Entity, IntoElement, ParentElement, Pixels, Styled, div, point, px};

use super::{FileList, ListState, row_index, row_number};
use crate::modifier_hints::{Cover, rows_to_lift};

impl FileList {
    #[must_use]
    pub fn lift_above(mut self, cover: Option<Cover>) -> Self {
        self.cover = cover;
        self
    }
}

pub(super) fn lift_for(
    state: &Entity<ListState>,
    (cursor, cover): (u64, Option<Cover>),
    row_height: Pixels,
    cx: &mut App,
) -> Pixels {
    let lift = cover.map_or(px(0.), |cover| {
        let state = state.read(cx);
        let Some(list) = state.list_bounds else {
            return px(0.);
        };
        let top = row_number(state.scroll.top_row(row_height));
        let Some(above) = cursor.checked_sub(top) else {
            return px(0.);
        };
        let bottom = list.origin.y + row_height * (above + 1) as f32;
        let rows = rows_to_lift(bottom, cover, row_height).min(row_index(above));
        row_height * rows as f32
    });
    state.update(cx, |state, _| state.lift = lift);
    lift
}

pub(super) fn lifted(list: impl IntoElement, lift: Pixels) -> impl IntoElement {
    div()
        .relative()
        .top(-lift)
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .size_full()
        .child(list)
}

pub(super) fn unshifted(bounds: Bounds<Pixels>, lift: Pixels) -> Bounds<Pixels> {
    Bounds {
        origin: point(bounds.origin.x, bounds.origin.y + lift),
        size: bounds.size,
    }
}
