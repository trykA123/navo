use gpui_kit::{
    AnyWindowHandle, App, AppContext, Bounds, Entity, Point, Render, TestAppContext, Window,
    WindowBounds, WindowOptions, px, size,
};

use crate::tokens::{self, Mode};

pub(crate) fn open<V: Render>(
    cx: &mut TestAppContext,
    build: impl FnOnce(&mut Window, &mut App) -> V,
) -> (AnyWindowHandle, Entity<V>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        tokens::apply(Mode::Light, cx);
        crate::file_list::bind_keys(cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: size(px(800.), px(600.)),
            })),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            let view = build(window, cx);
            cx.new(|_| view)
        })
        .expect("open test window")
    })
}
