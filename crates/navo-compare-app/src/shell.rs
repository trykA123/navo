//! The window's shell: the title bar, sidebar, topbar, active stage, toast host, and status bar.
//! It owns the tab list, syncs the sidebar, and handles browser-style tab keys (NS11, NS15, NS16),
//! the sidebar toggle, F6 pane cycling and hover-tip Escape forwarding.

use std::rc::Rc;

use gpui_kit::base::TestSupportExt as _;
use gpui_kit::{App, FocusHandle, Subscription, WindowAppearance};
use navo_compare_profile::appearance::PaletteId;
use navo_compare_profile::brand;
use navo_compare_profile::recent::{StartRow, StartRows};
use sidewise_kit::prelude::*;
use sidewise_kit::title_bar::NavoTitleBar;
use sidewise_kit::tokens;
use sidewise_l10n::t;

use crate::chrome::sidebar::{DismissTip, Sidebar, SidebarEvent, ToggleRail};
use crate::chrome::status_bar::{StatusBar, StatusHint, StatusNotice};
use crate::chrome::topbar_status::{
    Action as TopbarAction, Crumbs, Topbar, TopbarMode, ViewMenu as TopbarViewKey,
};
use crate::layers::keys_sheet;
use crate::layers::toast::{self, ToastHost};
use crate::menu_host::{MenuSlot, menu_view};
use crate::shell_prefs::theme_choice;
use crate::theme::{SidewiseTheme, apply_sidewise, mode_of};
use crate::theme_data::palettes::DIMS;
use crate::{updates, updates_check};

#[path = "shell_keys.rs"]
mod shell_keys;
#[path = "shell_layers.rs"]
mod shell_layers;
#[path = "shell_menus.rs"]
mod shell_menus;
#[path = "shell_tabs.rs"]
mod shell_tabs;
pub use shell_keys::bind_keys;
use shell_keys::{CONTEXT, CloseTab, CyclePane, NewTab, NextTab, PrevTab, ThemeMenu};
use shell_layers::{Layers, topbar_router};
use shell_menus::CloseOthers;
pub use shell_tabs::ShellTab;
use shell_tabs::{start_tab, tab_titles};

/// The shell view: the tab list, active index, sidebar, topbar, toast host, and focus handle.
pub struct Shell {
    tabs: Vec<ShellTab>,
    active: usize,
    sidebar: Entity<Sidebar>,
    _sidebar: Subscription,
    topbar: Entity<Topbar>,
    toasts: Entity<ToastHost>,
    layers: Layers,
    menu: MenuSlot,
    /// The sidebar Recent row whose menu is open, and the watch that clears it when that menu
    /// closes; its chosen action runs on this row.
    menu_row: Option<(StartRow, Subscription)>,
    handle: FocusHandle,
    _appearance: Subscription,
}

impl Shell {
    /// Applies the saved palette and mode (NC152); with no saved preferences, the current Sidewise
    /// palette (Violet if none) in the OS `appearance` mode.
    pub(crate) fn apply_appearance(appearance: WindowAppearance, cx: &mut App) {
        let (palette, mode) = theme_choice(appearance, cx).unwrap_or_else(|| {
            let palette = cx
                .try_global::<SidewiseTheme>()
                .map(|theme| theme.palette)
                .unwrap_or(PaletteId::Violet);
            (palette, mode_of(appearance))
        });
        apply_sidewise(palette, mode, cx);
    }

    /// Makes a shell with one start tab and focuses that tab's left field.
    /// It applies the active Sidewise palette in the window's mode and follows later changes (NS19).
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::apply_appearance(window.appearance(), cx);
        let appearance = cx.observe_window_appearance(window, |_, window, cx| {
            Self::apply_appearance(window.appearance(), cx);
            cx.notify();
        });
        let tabs = vec![start_tab(window, cx)];
        let titles = tab_titles(&tabs);
        let sidebar = cx.new(|cx| Sidebar::new(StartRows::default(), titles, cx));
        let sidebar_sub = cx.subscribe_in(&sidebar, window, |this, _, event, window, cx| {
            this.on_sidebar(*event, window, cx);
        });
        let topbar = start_topbar(window, cx);
        let toasts = cx.new(|_| ToastHost::new());
        toast::install_sink(&toasts, cx);
        let layers = Layers::new(window, cx);
        let shell = Self {
            tabs,
            active: 0,
            sidebar,
            _sidebar: sidebar_sub,
            topbar,
            toasts,
            layers,
            menu: None,
            menu_row: None,
            handle: cx.focus_handle(),
            _appearance: appearance,
        };
        shell.activated(window, cx);
        shell
    }

    /// The sidebar entity (lets key tests read its rail state).
    #[cfg(test)]
    pub(crate) fn sidebar(&self) -> &Entity<Sidebar> {
        &self.sidebar
    }

    /// Routes typed sidebar actions to tab mutations or redraws on rail transitions.
    fn on_sidebar(&mut self, event: SidebarEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            SidebarEvent::NewCompare => self.new_tab(window, cx),
            SidebarEvent::SelectTab(index) => self.select_tab(index, window, cx),
            SidebarEvent::CloseTab(index) => self.close_tab(index, window, cx),
            SidebarEvent::RailToggled(_) => cx.notify(),
            SidebarEvent::TabMenu(_, at) => self.show_tab_menu(at, window, cx),
            SidebarEvent::RecentMenu(index, at) => self.show_recent_menu((index, at), window, cx),
        }
    }

    /// Right-hand workspace column stacking the topbar, active stage, and status bar.
    fn main_pane(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("main-pane")
            .test_support()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(self.topbar.clone())
            .child(self.body(cx))
            .child(self.status_bar(cx))
    }

    /// The active stage and bottom-right toast overlay slot; empty toasts do not cover inputs.
    fn body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let background = tokens::current(cx).colors.background;
        let toasts = div()
            .id("toast-slot")
            .test_support()
            .absolute()
            .bottom(DIMS.inset_y)
            .right(DIMS.inset_x)
            .child(self.toasts.clone());
        div()
            .relative()
            .flex_1()
            .w_full()
            .min_h_0()
            .bg(background)
            .child(self.stage())
            .child(toasts)
    }

    /// Derives the status bar from active tab title, open-tab count, and working shortcuts.
    fn status_bar(&self, cx: &App) -> StatusBar {
        let count = self.tabs.len();
        let noun = if count == 1 { "open tab" } else { "open tabs" };
        let title = self
            .tabs
            .get(self.active)
            .map_or("", |tab| tab.title.as_ref());
        let summary = format!("{title} · {count} {noun}");
        let hints = [
            StatusHint {
                id: "hint-new".into(),
                keys: "ctrl-t".into(),
                label: "New tab".into(),
            },
            StatusHint {
                id: "hint-close".into(),
                keys: "ctrl-w".into(),
                label: "Close tab".into(),
            },
            StatusHint {
                id: "hint-sidebar".into(),
                keys: "ctrl-\\".into(),
                label: "Sidebar".into(),
            },
        ];
        let notice = updates::notice(cx).map(|v| StatusNotice {
            text: t!("update-available", version = v.to_string()).into(),
            on_open: Rc::new(updates_check::open_release_page),
            on_hide: Rc::new(updates_check::dismiss),
        });
        StatusBar::new(summary, hints).notice(notice)
    }
}

/// Builds the start-mode topbar: Keys opens the sheet; Sync is off (read-only).
fn start_topbar(window: &mut Window, cx: &mut Context<Shell>) -> Entity<Topbar> {
    let crumbs = Crumbs {
        title: "New compare".into(),
        detail: shell_tabs::START_HINT.into(),
    };
    let router = topbar_router(cx.weak_entity(), window);
    cx.new(|cx| {
        let mut bar = Topbar::new(crumbs, router, window, cx);
        bar.set_mode(TopbarMode::Start, cx);
        bar.set_available(TopbarAction::Sync, false, cx);
        bar
    })
}

impl Render for Shell {
    /// Draws the full-width title bar above the sidebar and main workspace column.
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx);
        let (background, text) = (t.colors.background, t.colors.text);
        let (font, size) = (t.typography.ui_font.clone(), t.typography.body);
        let root = shell_keys::wire_refresh(div().key_context(CONTEXT), &self.handle, cx);
        shell_menus::recent_actions(keys_sheet::wire(root, &self.layers.keys), cx)
            .relative()
            .on_action(cx.listener(|this, _: &NewTab, window, cx| this.new_tab(window, cx)))
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| {
                this.close_tab(this.active, window, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseOthers, window, cx| {
                this.close_others(window, cx);
            }))
            .on_action(cx.listener(|this, _: &NextTab, window, cx| this.next_tab(window, cx)))
            .on_action(cx.listener(|this, _: &PrevTab, window, cx| this.prev_tab(window, cx)))
            .on_action(cx.listener(|this, _: &ToggleRail, _, cx| {
                this.sidebar
                    .update(cx, |sidebar, cx| sidebar.toggle_rail(cx));
            }))
            .on_action(cx.listener(|this, _: &CyclePane, window, cx| this.cycle_pane(window, cx)))
            .on_action(cx.listener(|this, _: &DismissTip, _, cx| this.dismiss_tip(cx)))
            .on_action(cx.listener(|this, _: &TopbarViewKey, window, cx| {
                this.on_view_key(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ThemeMenu, window, cx| {
                if this.topbar.read(cx).is_available(TopbarAction::Theme) {
                    this.toggle_theme_menu(window, cx);
                }
            }))
            .flex()
            .flex_col()
            .size_full()
            .bg(background)
            .text_color(text)
            .font_family(font)
            .text_size(size)
            .child(NavoTitleBar::new(brand::NAME).menu_arrow(false))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(self.sidebar.clone())
                    .child(self.main_pane(cx)),
            )
            .child(self.layers.keys.clone())
            .children(self.view_backdrop(cx))
            .children(self.theme_backdrop(cx))
            .children(menu_view(&self.menu))
    }
}
