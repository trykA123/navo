use std::rc::Rc;

use rusty_core::{Location, ProviderId, Selection, ViewState};
use rusty_fs::FsEntry;
use rusty_ui::app_menu::{AppMenu, Identity, Install, QuickSettings};
use rusty_ui::file_list::{FileList, FileListProps, SortKey};
use rusty_ui::fkey_bar::{FKeyBar, KeyLayer};
use rusty_ui::pane_chrome::{
    DriveInfo, DrivePill, PaneTabs, PathBar, card_header, current_drive, pane_card,
};
use rusty_ui::prelude::*;
use rusty_ui::primitives::{Splitter, SplitterState};
use rusty_ui::side_sheet::{SheetFrame, SheetState, Side as PaneSide};
use rusty_ui::status_bar::StatusBar;
use rusty_ui::tokens::{self, Density, Mode, Theme, ThemeChoice};
use rusty_ui::window_view::{KEY_WARNING, key_labels, layer_keys};
use rusty_ui::{FerriteTitleBar, HeaderSearch};

use crate::drives_page::six_drives;
use crate::gallery::Page;
use crate::pane_page::status_of;
use crate::pane_sample;
use crate::sheet_sample;

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

pub struct SheetShows {
    pub active: PaneSide,
    pub window_width: f32,
    pub is_ringed: bool,
}

struct Sheet {
    shows: SheetShows,
    state: Entity<SheetState>,
}

pub struct WindowPage {
    held: Option<KeyLayer>,
    has_job: bool,
    is_menu_open: bool,
    sheet: Option<Sheet>,
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
            is_menu_open: false,
            sheet: None,
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

    pub fn with_menu(cx: &mut Context<Self>) -> Self {
        Self {
            is_menu_open: true,
            ..Self::new(None, false, cx)
        }
    }

    pub fn with_sheet(shows: SheetShows, cx: &mut Context<Self>) -> Self {
        let state = cx.new(|cx| {
            let mut state = SheetState::new();
            state.open(cx);
            state
        });
        Self {
            sheet: Some(Sheet { shows, state }),
            ..Self::new(None, false, cx)
        }
    }

    fn active_index(&self) -> usize {
        match self.sheet.as_ref().map(|sheet| sheet.shows.active) {
            Some(PaneSide::Right) => 1,
            _ => 0,
        }
    }

    fn sheet_layer(&self, t: &Theme) -> Option<impl IntoElement + use<>> {
        let sheet = self.sheet.as_ref()?;
        let body = px(sheet.shows.window_width) - t.spacing.md * 2.;
        let passive_card = (body - t.spacing.md) / 2.;
        let frame = SheetFrame::new(body, passive_card, sheet.shows.active);
        let layer = div()
            .absolute()
            .top(t.spacing.xs)
            .bottom(t.spacing.xs)
            .left(t.spacing.md)
            .right(t.spacing.md)
            .child(sheet_sample::sheet(t, &sheet.state, frame).accent_ring(sheet.shows.is_ringed));
        Some(layer)
    }

    fn menu(&self, t: &Theme) -> Option<AnyElement> {
        let theme = match t.mode {
            Mode::Light => ThemeChoice::Light,
            Mode::Dark => ThemeChoice::Dark,
        };
        let settings = QuickSettings {
            theme,
            density: Density::Normal,
            show_hidden: true,
        };
        let identity = Identity {
            version: "0.0.1".into(),
            install: Install::Portable,
            folder: r"D:\Apps\Ferrite".into(),
        };
        self.is_menu_open
            .then(|| AppMenu::new("app-menu", identity, settings).into_overlay(t))
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
            is_active: index == self.active_index(),
            filter: None,
            sort: SortKey::default(),
        };
        FileList::new(("list", index), props)
    }

    fn card(&self, index: usize, cx: &App) -> impl IntoElement {
        let side = &SIDES[index];
        let is_active = index == self.active_index();
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
        let label = if index == 0 {
            "Left pane"
        } else {
            "Right pane"
        };
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
        let is_open = self.is_menu_open;
        let menu = self.menu(&t);
        let sheet = self.sheet_layer(&t);
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(t.surfaces.mica())
            .child(
                FerriteTitleBar::new("Ferrite")
                    .on_mica()
                    .app_button(is_open, |_, _, _| {})
                    .header(HeaderSearch::new()),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .px(t.spacing.xs)
                    .child(panes)
                    .children(sheet),
            )
            .child(self.fkey_bar())
            .children(menu)
    }
}

pub const NARROW: f32 = 1100.;
pub const WIDE: f32 = 1440.;

fn fixed(width: f32, window: &Entity<WindowPage>) -> AnyElement {
    div()
        .w(px(width))
        .h_full()
        .flex_none()
        .child(window.clone())
        .into_any_element()
}

pub fn window_element(page: Page, windows: [&Entity<WindowPage>; 8]) -> Option<AnyElement> {
    let [main, shift, alt, job, menu, sheet, sheet_left, sheet_narrow] = windows;
    let shown = match page {
        Page::Window => main,
        Page::WindowShift => shift,
        Page::WindowAlt => alt,
        Page::WindowJob => job,
        Page::AppMenu => menu,
        Page::WindowNarrow => return Some(fixed(NARROW, main)),
        Page::Sheet => return Some(fixed(WIDE, sheet)),
        Page::SheetLeft => return Some(fixed(WIDE, sheet_left)),
        Page::SheetNarrow => return Some(fixed(NARROW, sheet_narrow)),
        _ => return None,
    };
    Some(shown.clone().into_any_element())
}
