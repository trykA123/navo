use gpui_kit::{FocusHandle, Stateful};

use super::TableView;
use crate::prelude::*;
use crate::window_view::viewer::search::view_find::{
    Findable, ViewFind, close_find, contains_ignore_case, find_again, open_find,
};
use crate::window_view::viewer::{Back, OpenFind};

impl Findable for TableView {
    type Hit = (usize, usize);

    fn find_slot(&mut self) -> &mut Option<ViewFind<Self::Hit>> {
        &mut self.find
    }

    fn hits_for(&self, query: &str) -> (Vec<Self::Hit>, usize) {
        let hits: Vec<Self::Hit> = (0..self.rows.len())
            .filter_map(|row| Some((row, self.rows.get(row)?.1)))
            .flat_map(|(row, cells)| {
                let found = cells
                    .iter()
                    .enumerate()
                    .filter(|(_, cell)| contains_ignore_case(cell, query));
                found.map(move |(column, _)| (row, column))
            })
            .collect();
        let start = hits.iter().position(|hit| *hit >= self.cursor);
        (hits, start.unwrap_or(0))
    }

    fn show_hit(&mut self, hit: Self::Hit, cx: &mut Context<Self>) {
        self.move_to(hit.0, hit.1, cx);
    }

    fn body_focus(&self) -> FocusHandle {
        self.focus.clone()
    }
}

impl TableView {
    pub(super) fn find_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &OpenFind, window, cx| open_find(view, window, cx)))
            .on_action(cx.listener(|view, _: &Back, window, cx| {
                if !close_find(view, window, cx) {
                    cx.propagate();
                }
            }))
    }

    pub(super) fn found_cell(&self, cell: (usize, usize)) -> Option<bool> {
        let find = self.find.as_ref()?;
        find.is_hit(cell).then(|| find.is_current(cell))
    }

    pub(super) fn find_next(
        &mut self,
        backward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        find_again(self, backward, window, cx);
    }
}
