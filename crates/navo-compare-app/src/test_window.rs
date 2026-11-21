//! Test-only helper: opens an 800 x 600 themed window around one view, for the view tests.

use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, Context, Entity, Point, Render, TestAppContext, Window,
    WindowBounds, WindowOptions, px, size,
};
use sidewise_kit::tokens::{self, Mode};

/// Opens a test window holding the view `build` makes; returns the window handle and the view.
/// Fails (test panic) when the window cannot open.
pub(crate) fn open<V: Render>(
    cx: &mut TestAppContext,
    build: impl FnOnce(&mut Window, &mut Context<V>) -> V,
) -> (AnyWindowHandle, Entity<V>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        tokens::apply(Mode::Light, cx);
        crate::shell::bind_keys(cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: size(px(800.), px(600.)),
            })),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| build(window, cx)))
            .expect("open test window")
    })
}
