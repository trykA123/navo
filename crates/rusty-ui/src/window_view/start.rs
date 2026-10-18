use std::rc::Rc;

use super::{FerriteWindow, StartOptions, bind_app_keys};
use crate::prelude::*;

pub fn run(mut options: StartOptions, on_quit: Rc<dyn Fn()>) {
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
        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some("ferrite".into()),
            ..crate::title_bar::main_window_options("Ferrite")
        };
        let opened = crate::open_window(window_options, cx, move |window, cx| {
            cx.new(|cx| FerriteWindow::new(options, window, cx))
        });
        if opened.is_err() {
            cx.quit();
        }
    });
}
