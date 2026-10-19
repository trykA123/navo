mod card;

use std::rc::Rc;

use rusty_core::{Location, ProviderId};
use rusty_fs::FsEntry;
use rusty_ui::app_menu::{AppMenu, Identity, Install, QuickSettings};
use rusty_ui::commands::Keymap;
use rusty_ui::context_menu::{ContextMenu, ContextMenuInput};
use rusty_ui::crash_notice::{CrashKeys, CrashNotice};
use rusty_ui::file_list::SortKey;
use rusty_ui::fkey_bar::{FKeyBar, KeyLayer};
use rusty_ui::job_card::{JobCard, JobPill, PillFit};
use rusty_ui::modifier_hints::{Combo, Cover, HintsDrawer, Layer, drawer_height, layer_where};
use rusty_ui::pane_chrome::DriveInfo;
use rusty_ui::prelude::*;
use rusty_ui::primitives::{Button, ButtonVariant, Splitter, SplitterState};
use rusty_ui::settings::{Field, Values, WindowParts};
use rusty_ui::shortcuts::ShortcutsPage;
use rusty_ui::side_sheet::{SheetFrame, SheetState, Side as PaneSide};
use rusty_ui::tokens::{self, Density, Mode, Theme, ThemeChoice};
use rusty_ui::window_view::{
    DELETE_WARNING, delete_warning_in, key_labels, keys_in_layer, layer_keys, settings_labels_in,
    window_view_menu,
};
use rusty_ui::{FerriteTitleBar, HeaderSearch};

use crate::drives_page::six_drives;
use crate::job_sample;
use crate::pane_sample;
use crate::settings_sample;
use crate::sheet_sample;
use crate::shortcuts_sample;

#[derive(Default)]
struct ViewMenuShown {
    wanted: bool,
    frames: u8,
    menu: Option<Entity<ContextMenu>>,
}

const FRAMES_BEFORE_MENU: u8 = 2;

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
    parts: WindowParts,
    sort: SortKey,
    is_single_tab: bool,
    view_menu: ViewMenuShown,
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
            parts: WindowParts::default(),
            sort: SortKey::default(),
            is_single_tab: false,
            view_menu: ViewMenuShown::default(),
        }
    }

    pub fn sorted_by(mut self, sort: SortKey) -> Self {
        self.sort = sort;
        self
    }

    pub fn with_parts(parts: WindowParts, shows_menu: bool, cx: &mut Context<Self>) -> Self {
        Self {
            parts,
            is_single_tab: true,
            view_menu: ViewMenuShown {
                wanted: shows_menu,
                ..ViewMenuShown::default()
            },
            ..Self::new(None, cx)
        }
    }

    fn shows_settings(&self) -> bool {
        self.sheet
            .as_ref()
            .is_some_and(|sheet| sheet.shows.settings.is_some() || sheet.shows.shortcuts.is_some())
    }

    fn fkey_bar(&self, window: &Window) -> Option<FKeyBar> {
        if self.shows_settings() {
            let labels = settings_labels_in(Keymap::builtin());
            return Some(
                FKeyBar::new("fkeys", keys_in_layer(&labels, self.held)).layers(self.held, labels),
            );
        }
        if !self.parts.fkey_bar {
            return None;
        }
        let bar = FKeyBar::new("fkeys", layer_keys(self.held)).layers(self.held, key_labels());
        let bar = match delete_warning_in(Keymap::builtin()) {
            Some((layer, index)) => bar.warn(layer, index, DELETE_WARNING),
            None => bar,
        };
        let Some(shown) = self.jobs else {
            return Some(bar);
        };
        let fit = PillFit::for_width(window.viewport_size().width);
        let bar = bar.reserve_pill_width(fit.slot_width());
        Some(if shown.is_pill {
            bar.right_slot(JobPill::new(&job_sample::jobs(shown.count), fit))
        } else {
            bar
        })
    }

    fn open_view_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let viewport = window.viewport_size();
        let t = tokens::current(cx).clone();
        let input = ContextMenuInput {
            menu: window_view_menu(&Values::default()),
            context: "View".into(),
            anchor: Point::new(viewport.width - t.spacing.xl, t.spacing.header_height),
            bounds: Bounds::new(Point::default(), viewport),
            restore: None,
        };
        self.view_menu.menu = Some(cx.new(|cx| {
            let mut menu = ContextMenu::new(input, window, cx);
            menu.hide_classic(cx);
            menu.name_ferrite_group("View", cx);
            menu
        }));
        cx.notify();
    }

    fn view_menu_layer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<ContextMenu>> {
        let shown = &mut self.view_menu;
        if shown.wanted && shown.menu.is_none() {
            if shown.frames < FRAMES_BEFORE_MENU {
                shown.frames += 1;
                window.request_animation_frame();
            } else {
                cx.defer_in(window, Self::open_view_menu);
            }
        }
        shown.menu.clone()
    }

    fn title_bar(&self, is_menu_open: bool) -> FerriteTitleBar {
        let bar = FerriteTitleBar::new("Ferrite")
            .on_mica()
            .app_button(is_menu_open, |_, _, _| {})
            .trailing(
                Button::new("window-view", "View")
                    .variant(ButtonVariant::Ghost)
                    .tooltip("View menu (Alt+V)"),
            );
        if self.parts.title_search {
            bar.header(HeaderSearch::new())
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

    fn hint_layer(&self, combo: Combo) -> Layer {
        let has_crash = self.has_crash;
        layer_where(Keymap::builtin(), combo, &|spec| {
            spec.action.is_some() && (has_crash || spec.name != "crash_notice.focus")
        })
    }

    fn hints_layer(&self, t: &Theme) -> Option<impl IntoElement + use<>> {
        let shown = self.hints?;
        let drawer = HintsDrawer::new(self.hint_layer(shown.combo)).settings_key(Some("ctrl-,"));
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
        let layer = self.hint_layer(shown.combo);
        let bar = if self.parts.fkey_bar {
            t.spacing.fkey_height
        } else {
            px(0.)
        };
        let bottom = window.viewport_size().height - bar;
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
}

impl Render for WindowPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let (left, right) = (self.card(0, window, cx), self.card(1, window, cx));
        let panes = Splitter::new("panes", &self.split, left, right).line_on_hover();
        let is_open = self.is_menu_open;
        let menu = self.menu(&t);
        let sheet = self.sheet_layer(&t, window);
        let view_menu = self.view_menu_layer(window, cx);
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(t.surfaces.mica())
            .child(self.title_bar(is_open))
            .children(self.crash_notice(&t))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .px(t.spacing.xs)
                    .when(!self.parts.fkey_bar, |area| area.pb(t.spacing.md))
                    .child(panes)
                    .children(self.job_card(&t))
                    .children(sheet)
                    .children(self.hints_layer(&t)),
            )
            .children(self.fkey_bar(window))
            .children(menu)
            .children(view_menu)
    }
}
