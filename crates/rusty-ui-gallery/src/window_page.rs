use std::rc::Rc;

use rusty_core::{Location, ProviderId, Selection, ViewState};
use rusty_fs::FsEntry;
use rusty_ui::app_menu::{AppMenu, Identity, Install, QuickSettings};
use rusty_ui::commands::Keymap;
use rusty_ui::crash_notice::{CrashKeys, CrashNotice};
use rusty_ui::file_list::{FileList, FileListProps, SortKey};
use rusty_ui::fkey_bar::{FKeyBar, KeyLayer};
use rusty_ui::job_card::{JobCard, JobPill, PillFit};
use rusty_ui::modifier_hints::{Combo, Cover, HintsDrawer, drawer_height, layer_in};
use rusty_ui::pane_chrome::{
    DriveInfo, DrivePill, PaneTabs, PathBar, card_header, current_drive, pane_card,
};
use rusty_ui::prelude::*;
use rusty_ui::primitives::{Splitter, SplitterState};
use rusty_ui::settings::Field;
use rusty_ui::shortcuts::ShortcutsPage;
use rusty_ui::side_sheet::{SheetFrame, SheetState, Side as PaneSide};
use rusty_ui::status_bar::StatusBar;
use rusty_ui::tokens::{self, Density, Mode, Theme, ThemeChoice};
use rusty_ui::window_view::{
    DELETE_WARNING, delete_warning_in, key_labels, keys_in_layer, layer_keys, settings_labels_in,
};
use rusty_ui::{FerriteTitleBar, HeaderSearch};

use crate::drives_page::six_drives;
use crate::job_sample;
use crate::pane_page::status_of;
use crate::pane_sample;
use crate::settings_sample;
use crate::sheet_sample;
use crate::shortcuts_sample;

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
    pub window_width: Option<f32>,
    pub is_ringed: bool,
    pub settings: Option<Field>,
    pub shortcuts: Option<Entity<ShortcutsPage>>,
}

#[derive(Clone, Copy)]
pub struct HintsShown {
    pub combo: Combo,
    pub cursor: u64,
}

#[derive(Clone, Copy)]
pub struct JobsShown {
    pub count: usize,
    pub is_pill: bool,
}

struct Sheet {
    shows: SheetShows,
    state: Entity<SheetState>,
}

pub struct WindowPage {
    held: Option<KeyLayer>,
    hints: Option<HintsShown>,
    jobs: Option<JobsShown>,
    is_menu_open: bool,
    has_crash: bool,
    sheet: Option<Sheet>,
    rows: Rc<[FsEntry]>,
    drives: Rc<[DriveInfo]>,
    split: Entity<SplitterState>,
}

fn location(text: &str) -> Location {
    Location::new(text).unwrap_or_else(|_| Location::provider_root(ProviderId::local()))
}

impl WindowPage {
    pub fn new(held: Option<KeyLayer>, cx: &mut Context<Self>) -> Self {
        Self {
            held,
            hints: None,
            jobs: None,
            is_menu_open: false,
            has_crash: false,
            sheet: None,
            rows: pane_sample::rows(),
            drives: six_drives().iter().take(4).cloned().collect(),
            split: cx.new(|_| SplitterState::new(0.5)),
        }
    }

    fn shows_settings(&self) -> bool {
        self.sheet
            .as_ref()
            .is_some_and(|sheet| sheet.shows.settings.is_some() || sheet.shows.shortcuts.is_some())
    }

    fn fkey_bar(&self, window: &Window) -> FKeyBar {
        if self.shows_settings() {
            let labels = settings_labels_in(Keymap::builtin());
            return FKeyBar::new("fkeys", keys_in_layer(&labels, self.held))
                .layers(self.held, labels);
        }
        let bar = FKeyBar::new("fkeys", layer_keys(self.held)).layers(self.held, key_labels());
        let bar = match delete_warning_in(Keymap::builtin()) {
            Some((layer, index)) => bar.warn(layer, index, DELETE_WARNING),
            None => bar,
        };
        let Some(shown) = self.jobs else {
            return bar;
        };
        let fit = PillFit::for_width(window.viewport_size().width);
        let bar = bar.reserve_pill_width(fit.slot_width());
        if shown.is_pill {
            bar.right_slot(JobPill::new(&job_sample::jobs(shown.count), fit))
        } else {
            bar
        }
    }

    fn job_card(&self, t: &Theme) -> Option<impl IntoElement + use<>> {
        let shown = self.jobs.filter(|shown| !shown.is_pill)?;
        let card = JobCard::new(job_sample::jobs(shown.count))
            .on_shrink(|_, _| {})
            .on_pause(|_, _, _| {})
            .on_resume(|_, _, _| {})
            .on_cancel(|_, _, _| {});
        Some(
            div()
                .absolute()
                .right(t.spacing.xxl)
                .bottom(t.spacing.md)
                .child(card),
        )
    }

    pub fn with_jobs(shown: JobsShown, cx: &mut Context<Self>) -> Self {
        Self {
            jobs: Some(shown),
            ..Self::new(None, cx)
        }
    }

    pub fn with_hints(shown: HintsShown, held: KeyLayer, cx: &mut Context<Self>) -> Self {
        Self {
            hints: Some(shown),
            ..Self::new(Some(held), cx)
        }
    }

    fn hints_layer(&self, t: &Theme) -> Option<impl IntoElement + use<>> {
        let shown = self.hints?;
        let drawer =
            HintsDrawer::new(layer_in(Keymap::builtin(), shown.combo)).settings_key(Some("ctrl-,"));
        Some(
            div()
                .absolute()
                .left(t.spacing.md)
                .right(t.spacing.md)
                .bottom_0()
                .child(drawer),
        )
    }

    fn cover(&self, index: usize, window: &Window, cx: &App) -> Option<Cover> {
        let shown = self.hints.filter(|_| index == self.active_index())?;
        let t = tokens::current(cx);
        let layer = layer_in(Keymap::builtin(), shown.combo);
        let bottom = window.viewport_size().height - t.spacing.fkey_height;
        Some(Cover {
            top: bottom - drawer_height(t, &layer),
            gap: t.spacing.md,
        })
    }

    pub fn with_crash(cx: &mut Context<Self>) -> Self {
        Self {
            has_crash: true,
            ..Self::new(None, cx)
        }
    }

    fn crash_notice(&self, t: &Theme) -> Option<impl IntoElement + use<>> {
        self.has_crash.then(|| {
            div()
                .flex_none()
                .mx(t.spacing.xs * 2.)
                .my(t.spacing.xs)
                .child(CrashNotice::new(CrashKeys::of(Keymap::builtin())))
        })
    }

    pub fn with_menu(cx: &mut Context<Self>) -> Self {
        Self {
            is_menu_open: true,
            ..Self::new(None, cx)
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
            ..Self::new(None, cx)
        }
    }

    fn active_index(&self) -> usize {
        match self.sheet.as_ref().map(|sheet| sheet.shows.active) {
            Some(PaneSide::Right) => 1,
            _ => 0,
        }
    }

    fn sheet_layer(&self, t: &Theme, window: &Window) -> Option<impl IntoElement + use<>> {
        let sheet = self.sheet.as_ref()?;
        let width = sheet
            .shows
            .window_width
            .map_or(window.viewport_size().width, px);
        let body = width - t.spacing.md * 2.;
        let passive_card = (body - t.spacing.md) / 2.;
        let frame = SheetFrame::new(body, passive_card, sheet.shows.active);
        let frame = if self.shows_settings() {
            frame.covering_passive_card()
        } else {
            frame
        };
        let shown = match (&sheet.shows.shortcuts, sheet.shows.settings) {
            (Some(page), _) => shortcuts_sample::sheet(&sheet.state, frame, page),
            (None, Some(field)) => settings_sample::sheet(t, &sheet.state, frame, field),
            (None, None) => sheet_sample::sheet(t, &sheet.state, frame),
        };
        let layer = div()
            .absolute()
            .top(t.spacing.xs)
            .bottom(t.spacing.xs)
            .left(t.spacing.md)
            .right(t.spacing.md)
            .child(shown.accent_ring(sheet.shows.is_ringed));
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
                cursor: self.hints.map_or(side.cursor, |shown| shown.cursor),
                scroll: 0,
            },
            marked,
            is_active: index == self.active_index(),
            filter: None,
            sort: SortKey::default(),
        };
        FileList::new(("list", index), props)
    }

    fn card(&self, index: usize, window: &Window, cx: &App) -> impl IntoElement {
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
            .child(
                self.list(index, side, Rc::new(marked.clone()))
                    .lift_above(self.cover(index, window, cx)),
            )
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let (left, right) = (self.card(0, window, cx), self.card(1, window, cx));
        let panes = Splitter::new("panes", &self.split, left, right).line_on_hover();
        let is_open = self.is_menu_open;
        let menu = self.menu(&t);
        let sheet = self.sheet_layer(&t, window);
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
            .children(self.crash_notice(&t))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .px(t.spacing.xs)
                    .child(panes)
                    .children(self.job_card(&t))
                    .children(sheet)
                    .children(self.hints_layer(&t)),
            )
            .child(self.fkey_bar(window))
            .children(menu)
    }
}
