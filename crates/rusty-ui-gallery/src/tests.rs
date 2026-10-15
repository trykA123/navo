use std::path::PathBuf;

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Bounds, Point, Role, TestAppContext, WindowBounds, WindowOptions, px, size};
use rusty_ui::tokens::{self, Mode};

use crate::gallery::Gallery;
use crate::options::{Options, parse_args};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|arg| (*arg).to_string()).collect()
}

#[test]
fn no_arguments_open_the_gallery_on_screen() {
    assert_eq!(parse_args(args(&[])), Ok(Options::default()));
}

#[test]
fn screenshot_takes_an_output_directory() {
    let parsed = parse_args(args(&["--screenshot", "design/screens"]));
    assert_eq!(
        parsed.map(|options| options.screenshot_dir),
        Ok(Some(PathBuf::from("design/screens")))
    );
}

#[test]
fn screenshot_without_a_directory_is_an_error() {
    assert!(parse_args(args(&["--screenshot"])).is_err());
    assert!(parse_args(args(&["--bogus"])).is_err());
}

#[gpui_kit::test]
fn the_gallery_opens_a_window_with_every_primitive(cx: &mut TestAppContext) {
    let (window, _) = cx.update(|cx| {
        rusty_ui::init(Mode::Light, cx).expect("bundled fonts load");
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: size(px(1280.), px(720.)),
            })),
            ..Default::default()
        };
        rusty_ui::open_window(options, cx, |_, cx| {
            cx.new(|cx| Gallery::new(Mode::Light, cx))
        })
        .expect("open the gallery window")
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        for id in [
            "copy",
            "theme-toggle",
            "key-search",
            "tabs",
            "rows",
            "split",
        ] {
            assert!(window.find(id).visible(), "{id} is not on screen");
        }
        assert_eq!(window.find("rows").role(), Some(Role::List));
        assert_eq!(window.find("tabs").role(), Some(Role::TabList));
        assert_eq!(
            window.find("theme-toggle").label(),
            Some("Switch to the dark theme")
        );
        window.click("theme-toggle", cx);
        assert_eq!(
            window.find("theme-toggle").label(),
            Some("Switch to the light theme")
        );
    })
    .expect("window");
    cx.update(|cx| assert_eq!(tokens::current(cx).mode, Mode::Dark));
}
