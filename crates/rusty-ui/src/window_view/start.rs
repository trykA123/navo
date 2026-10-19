use std::rc::Rc;

use rusty_core::AppState;

use super::placement::{self, DisplayScale, DisplayScales};
use super::{FerriteWindow, StartOptions, bind_app_keys};
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
            app_id: Some("ferrite".into()),
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
    let (display_id, bounds) = match placement::opening(&placement::screens(cx), placements) {
        Some((display, bounds)) => (Some(display), bounds),
        None => (
            None,
            WindowBounds::Windowed(Bounds::centered(None, size(px(1280.), px(800.)), cx)),
        ),
    };
    WindowOptions {
        window_bounds: Some(bounds),
        display_id,
        ..crate::title_bar::main_window_options("Ferrite")
    }
}
