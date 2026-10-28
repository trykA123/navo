use std::rc::Rc;

use rusty_core::AppState;

use super::placement::{self, DisplayScale, DisplayScales};
use super::{FerriteWindow, KeymapHooks, OpenHooks, StartOptions, bind_app_keys, settings_save};
use crate::browser::{Browser, RenameHooks};
use crate::prelude::*;

pub fn run(mut options: StartOptions, display_scale: DisplayScale, on_quit: Rc<dyn Fn()>) {
    crate::application().run(move |cx| {
        cx.on_app_quit(move |_| {
            let on_quit = on_quit.clone();
            async move { on_quit() }
        })
        .detach();
        if let Some(watch) = options.start_watch.as_mut() {
            watch.mark("app running");
        }
        let _ = crate::init(options.mode, cx);
        bind_app_keys(cx);
        cx.set_global(DisplayScales(display_scale));
        let window_options = WindowOptions {
            app_id: Some(rusty_core::brand::SLUG.into()),
            ..main_window_options(&options.state, cx)
        };
        let opened = crate::open_window(window_options, cx, move |window, cx| {
            cx.new(|cx| FerriteWindow::new(options, window, cx))
        });
        if opened.is_err() {
            cx.quit();
        }
    });
}

pub(super) fn main_window_options(state: &AppState, cx: &App) -> WindowOptions {
    let placements = state
        .window(state.focused_window())
        .map(rusty_core::Window::placement)
        .unwrap_or_default();
    let screens = placement::screens(cx);
    let (display_id, bounds) = match placement::opening(&screens, placements) {
        Some((display, bounds)) => {
            let bounds = crate::title_bar::at_least_minimum(bounds);
            let area = screens.iter().find(|screen| screen.id == display);
            let bounds = area.map_or(bounds, |screen| {
                placement::keep_on(bounds, screen.work_area)
            });
            (Some(display), bounds)
        }
        None => (
            None,
            WindowBounds::Windowed(Bounds::centered(None, size(px(1280.), px(800.)), cx)),
        ),
    };
    WindowOptions {
        display_id,
        ..crate::title_bar::main_window_options(rusty_core::brand::NAME, bounds)
    }
}

impl FerriteWindow {
    pub(super) fn browser(options: &StartOptions, cx: &mut Context<Self>) -> Browser {
        let state = options.state.clone();
        let mut browser = Browser::new(state, options.provider.clone(), Self::waker(cx));
        browser.set_operations(options.operations.clone());
        browser.set_launcher(options.launcher.clone());
        browser.set_listing_cache(options.listing_cache.clone(), options.cache_keys.clone());
        browser.set_folder_totals(options.search.folder_totals.clone());
        browser.set_retained_hooks(options.retained.clone());
        browser
    }
}

pub(super) type AfterOpen = (
    KeymapHooks,
    OpenHooks,
    Option<rusty_core::Location>,
    Option<RenameHooks>,
);

impl FerriteWindow {
    pub(super) fn after_open(
        mut self,
        (keymap, open, start_view, rename): AfterOpen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        settings_save::steer(&self.values, &mut self.browser);
        self.browser.start();
        self.browser.set_rename_hooks(rename);
        self.listen_files(keymap, window, cx);
        self.tell_pending_undo(cx);
        self.listen_open(open, window, cx);
        self.apply_theme(window, cx);
        Self::close_saves_and_quits(window, cx);
        Self::poll_drives(cx);
        Self::poll_index_activity(cx);
        self.check_index_upgrade(cx);
        cx.defer_in(window, move |view, window, cx| {
            view.focus_core_pane(window, cx);
            view.watch_listings(cx);
            if let Some(location) = start_view {
                view.view_location(location, window, cx);
            }
        });
        self
    }
}
