use std::rc::Rc;

use rusty_core::{Location, ProviderId, Selection, ViewState};
use rusty_fs::FsEntry;
use rusty_ui::file_list::{FileList, FileListProps, SortKey};
use rusty_ui::fkey_bar::{FKeyBar, KeyLayer};
use rusty_ui::pane_chrome::{
    DriveInfo, DrivePill, PaneTabs, PathBar, card_header, current_drive, pane_card,
};
use rusty_ui::prelude::*;
use rusty_ui::primitives::{Splitter, SplitterState};
use rusty_ui::status_bar::StatusBar;
use rusty_ui::tokens;
use rusty_ui::window_view::{KEY_WARNING, key_labels, layer_keys};
use rusty_ui::{FerriteTitleBar, HeaderSearch};

use crate::drives_page::six_drives;
use crate::gallery::Page;
use crate::pane_page::status_of;
use crate::pane_sample;

struct Side {
    tabs: &'static [&'static str],
    path: &'static str,
    cursor: u64,
    marked: &'static [u64],
}

const SIDES: [Side; 2] = [
    Side {
        tabs: &["commander", "Downloads", "Steam"],
        path: r"C:\work\commander",
        cursor: 9,
        marked: &[7, 11],
    },
    Side {
        tabs: &["2026-10", "photos"],
        path: r"Z:\archive\2026-10",
        cursor: 4,
        marked: &[],
    },
];

pub struct WindowPage {
    held: Option<KeyLayer>,
    has_job: bool,
    rows: Rc<[FsEntry]>,
    drives: Rc<[DriveInfo]>,
    split: Entity<SplitterState>,
}

fn location(text: &str) -> Location {
    Location::new(text).unwrap_or_else(|_| Location::provider_root(ProviderId::local()))
}

impl WindowPage {
    pub fn new(held: Option<KeyLayer>, has_job: bool, cx: &mut Context<Self>) -> Self {
        Self {
            held,
            has_job,
            rows: pane_sample::rows(),
            drives: six_drives().iter().take(4).cloned().collect(),
            split: cx.new(|_| SplitterState::new(0.5)),
        }
    }

    fn fkey_bar(&self) -> FKeyBar {
        let bar = FKeyBar::new("fkeys", layer_keys(self.held))
            .layers(self.held, key_labels())
            .warn(KEY_WARNING.0, KEY_WARNING.1, KEY_WARNING.2);
        if self.has_job {
            bar.reserve_pill()
        } else {
            bar
        }
    }

    fn list(&self, index: usize, side: &Side, marked: Rc<Selection>) -> FileList {
        let folder = side.tabs.first().copied().unwrap_or_default();
        let props = FileListProps {
            folder: folder.into(),
            rows: self.rows.clone(),
            view: ViewState {
                cursor: side.cursor,
                scroll: 0,
            },
            marked,
            is_active: index == 0,
            filter: None,
            sort: SortKey::default(),
        };
        FileList::new(("list", index), props)
    }

    fn card(&self, index: usize, cx: &App) -> impl IntoElement {
        let side = &SIDES[index];
        let is_active = index == 0;
        let here = location(side.path);
        let mut marked = Selection::new(self.rows.len() as u64);
        marked.mark_rows(side.marked.iter().copied());
        let tabs =
            PaneTabs::new(("tabs", index), side.tabs.iter().copied(), 0).highlighted(is_active);
        let header = card_header(tokens::current(cx))
            .child(div().flex_1().min_w_0().child(tabs))
            .child(DrivePill::new(
                ("drive-pill", index),
                current_drive(&self.drives, &here)
                    .and_then(|drive| self.drives.get(drive).cloned()),
            ));
        let label = if is_active { "Left pane" } else { "Right pane" };
        let card = pane_card(("pane", index), label, is_active, cx)
            .child(header)
            .child(PathBar::new(("path", index), here))
            .child(self.list(index, side, Rc::new(marked.clone())))
            .child(StatusBar::new(
                ("status", index),
                status_of(&self.rows, &marked),
            ));
        div()
            .size_full()
            .p(tokens::current(cx).spacing.xs)
            .child(card)
    }
}

impl Render for WindowPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let panes =
            Splitter::new("panes", &self.split, self.card(0, cx), self.card(1, cx)).line_on_hover();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(t.surfaces.mica())
            .child(
                FerriteTitleBar::new("Ferrite")
                    .on_mica()
                    .header(HeaderSearch::new()),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .px(t.spacing.xs)
                    .child(panes),
            )
            .child(self.fkey_bar())
    }
}

const NARROW: f32 = 1100.;

pub fn window_element(page: Page, windows: [&Entity<WindowPage>; 4]) -> Option<AnyElement> {
    let [main, shift, alt, job] = windows;
    let shown = match page {
        Page::Window => main,
        Page::WindowShift => shift,
        Page::WindowAlt => alt,
        Page::WindowJob => job,
        Page::WindowNarrow => {
            let narrow = div().w(px(NARROW)).h_full().child(main.clone());
            return Some(narrow.into_any_element());
        }
        _ => return None,
    };
    Some(shown.clone().into_any_element())
}
