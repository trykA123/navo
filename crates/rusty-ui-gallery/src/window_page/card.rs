use std::rc::Rc;

use rusty_core::{Selection, ViewState};
use rusty_ui::file_list::{FileList, FileListProps, ListParts};
use rusty_ui::pane_chrome::{DrivePill, PaneTabs, PathBar, card_header, current_drive, pane_card};
use rusty_ui::prelude::*;
use rusty_ui::status_bar::{FooterParts, StatusBar};
use rusty_ui::tokens;

use super::{SIDES, Side, WindowPage, location};
use crate::pane_page::status_of;

impl WindowPage {
    fn list(&self, index: usize, side: &Side, marked: Rc<Selection>) -> FileList {
        let folder = side.tabs.first().copied().unwrap_or_default();
        let props = FileListProps {
            folder: folder.into(),
            rows: self.rows.clone(),
            view: ViewState {
                cursor: self.hints.map_or(side.cursor, |shown| shown.cursor),
                scroll: 0,
            },
            marked,
            is_active: index == self.active_index(),
            filter: None,
            sort: self.sort,
        };
        let parts = self.parts;
        FileList::new(("list", index), props).parts(ListParts {
            ext: parts.ext_column,
            size: parts.size_column,
            date: parts.date_column,
            icons: parts.icons,
        })
    }

    fn header(&self, index: usize, side: &Side, cx: &App) -> Option<Div> {
        let parts = self.parts;
        let tabs: Vec<&str> = side
            .tabs
            .iter()
            .copied()
            .take(if self.is_single_tab { 1 } else { usize::MAX })
            .collect();
        let shows_tabs = parts.tabs_with_one || tabs.len() > 1;
        if !shows_tabs && !parts.drive_button {
            return None;
        }
        let here = location(side.path);
        let is_active = index == self.active_index();
        let tabs = PaneTabs::new(("tabs", index), tabs, 0).highlighted(is_active);
        let drive = DrivePill::new(
            ("drive-pill", index),
            current_drive(&self.drives, &here).and_then(|drive| self.drives.get(drive).cloned()),
        );
        let header = card_header(tokens::current(cx));
        let header = if shows_tabs {
            header.child(div().flex_1().min_w_0().child(tabs))
        } else {
            header.justify_end()
        };
        Some(header.when(parts.drive_button, |header| header.child(drive)))
    }

    pub(super) fn card(&self, index: usize, window: &Window, cx: &App) -> impl IntoElement {
        let side = &SIDES[index];
        let is_active = index == self.active_index();
        let mut marked = Selection::new(self.rows.len() as u64);
        marked.mark_rows(side.marked.iter().copied());
        let label = if index == 0 {
            "Left pane"
        } else {
            "Right pane"
        };
        let parts = self.parts;
        let footer =
            StatusBar::new(("status", index), status_of(&self.rows, &marked)).parts(FooterParts {
                free_space: parts.free_space,
                marked_totals: parts.marked_totals,
                contents: parts.contents,
            });
        let card = pane_card(("pane", index), label, is_active, cx)
            .children(self.header(index, side, cx))
            .when(parts.path_bar, |card| {
                card.child(PathBar::new(("path", index), location(side.path)))
            })
            .child(
                self.list(index, side, Rc::new(marked.clone()))
                    .lift_above(self.cover(index, window, cx)),
            )
            .children((!footer.is_empty()).then_some(footer));
        div()
            .size_full()
            .p(tokens::current(cx).spacing.xs)
            .child(card)
    }
}
