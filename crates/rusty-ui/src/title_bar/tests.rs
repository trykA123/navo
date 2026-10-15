use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::AppContext as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, Bounds, Context, IntoElement, ParentElement, Point, Render, Role, Styled,
    TestAppContext, Window, WindowBounds, WindowOptions, div, px, size,
};

use super::{FerriteTitleBar, HeaderSearch, window_options};
use crate::tokens::{self, Mode};

const WIDTH: f32 = 1000.;

struct Shell {
    closes: Rc<Cell<usize>>,
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let closes = self.closes.clone();
        let bar = FerriteTitleBar::new("Ferrite")
            .header(HeaderSearch::new())
            .on_close(move |_, _| closes.set(closes.get() + 1));
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(bar)
            .child(div().flex_1())
    }
}

fn open(cx: &mut TestAppContext, closes: Rc<Cell<usize>>) -> AnyWindowHandle {
    cx.update(|cx| {
        gpui_kit::init(cx);
        tokens::apply(Mode::Light, cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: size(px(WIDTH), px(600.)),
            })),
            ..window_options("Ferrite")
        };
        gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| Shell { closes }))
            .expect("open test window")
            .0
    })
}

#[test]
fn window_options_hide_the_native_caption_and_keep_the_title() {
    let options = window_options("Ferrite");
    let titlebar = options.titlebar.expect("title bar options");
    assert_eq!(titlebar.title.as_deref(), Some("Ferrite"));
    assert!(titlebar.appears_transparent);
    assert!(options.app_owns_titlebar_drag);
}

#[gpui_kit::test]
fn the_title_bar_shows_the_icon_title_and_search(cx: &mut TestAppContext) {
    let window = open(cx, Rc::default());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let bar = window.find("title-bar");
        assert!(bar.visible());
        assert_eq!(bar.bounds().size.width, px(WIDTH));
        assert_eq!(
            bar.bounds().size.height,
            tokens::current(cx).spacing.header_height
        );
        let title = window.find("title-bar-title");
        assert_eq!(title.role(), Some(Role::Label));
        assert_eq!(title.label(), Some("Ferrite"));
        let icon = window.find("title-bar-icon");
        assert!(icon.visible());
        assert_eq!(icon.bounds().size, size(px(24.), px(24.)));
        let search = window.find("title-bar-search");
        assert_eq!(
            search.label(),
            Some("Find anywhere: files, folders, commands")
        );
        assert!(search.bounds().left() > title.bounds().right());
        assert_eq!(window.find("title-bar-search-key").label(), Some("Ctrl+K"));
    })
    .expect("window");
}

#[gpui_kit::test]
fn header_content_neither_moves_nor_zooms_the_window(cx: &mut TestAppContext) {
    let window = open(cx, Rc::default());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.drag_to("title-bar-search", "title-bar-search-key", cx);
        window.double_click("title-bar-search", cx);
        assert!(window.find("title-bar-header").visible());
    })
    .expect("window");
}

#[gpui_kit::test]
fn window_buttons_use_the_theme_colours(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        for mode in [Mode::Light, Mode::Dark] {
            tokens::apply(mode, cx);
            let colors = tokens::current(cx).colors;
            let kit = cx.theme();
            assert_eq!(kit.foreground, colors.text, "{mode:?} glyphs");
            assert_eq!(kit.secondary_hover, colors.hover, "{mode:?} hover");
            let close = kit.danger;
            let is_red = close.h < 0.05 || close.h > 0.95;
            assert!(is_red && close.s > 0.5, "{mode:?} close hover {close:?}");
        }
    });
}

#[cfg(target_os = "windows")]
#[gpui_kit::test]
fn the_title_bar_ends_with_minimise_maximise_and_close(cx: &mut TestAppContext) {
    let window = open(cx, Rc::default());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let controls = window.within("window-controls");
        let minimise = controls.find("minimize").bounds();
        let maximise = controls.find("maximize").bounds();
        let close = controls.find("close").bounds();
        assert!(minimise.right() <= maximise.left());
        assert!(maximise.right() <= close.left());
        assert_eq!(close.right(), px(WIDTH));
    })
    .expect("window");
}

#[cfg(target_os = "linux")]
#[gpui_kit::test]
fn server_decorations_leave_the_window_buttons_to_the_window_manager(cx: &mut TestAppContext) {
    let window = open(cx, Rc::default());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("title-bar").visible());
        assert!(window.try_find("close").is_none());
    })
    .expect("window");
}
