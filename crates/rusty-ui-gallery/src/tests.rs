use std::path::PathBuf;

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Bounds, Point, Role, TestAppContext, WindowBounds, WindowOptions, px, size};
use rusty_ui::tokens::{self, Mode};

use crate::gallery::{Gallery, Page};
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

#[test]
fn pages_limit_the_screenshots_to_the_listed_stems() {
    let parsed = parse_args(args(&[
        "--screenshot",
        "shots",
        "--pages",
        "p7-pane,p15-table",
    ]));
    assert_eq!(
        parsed.map(|options| options.pages),
        Ok(Some(vec!["p7-pane".to_string(), "p15-table".to_string()]))
    );
    let all = parse_args(args(&["--screenshot", "shots"]));
    assert_eq!(all.map(|options| options.pages), Ok(None));
}

#[test]
fn an_unknown_page_stem_is_an_error_naming_it() {
    let error = parse_args(args(&["--screenshot", "shots", "--pages", "p7-pane,nope"]))
        .expect_err("unknown stem");
    assert!(error.contains("nope"));
    assert!(!error.contains("p7-pane"));
    assert!(parse_args(args(&["--pages", "p7-pane"])).is_err());
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
        rusty_ui::open_window(options, cx, |window, cx| {
            cx.new(|cx| Gallery::new(Mode::Light, window, cx))
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
            "title-bar-title",
            "title-bar-search",
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

#[gpui_kit::test]
fn the_pane_page_shows_one_pane_of_sample_rows(cx: &mut TestAppContext) {
    let (window, _) = cx.update(|cx| {
        rusty_ui::init(Mode::Light, cx).expect("bundled fonts load");
        rusty_ui::file_list::bind_keys(cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: size(px(1280.), px(720.)),
            })),
            ..Default::default()
        };
        rusty_ui::open_window(options, cx, |window, cx| {
            cx.new(|cx| Gallery::new(Mode::Light, window, cx))
        })
        .expect("open the gallery window")
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("Pane", cx);
        for id in [
            "pane-tabs",
            "pane-path",
            "pane-list",
            "pane-status",
            "pane-fkeys",
        ] {
            assert!(window.try_find(id).is_some(), "{id} is missing");
        }
        assert_eq!(
            window.within("pane-list").find("list").label(),
            Some("core files")
        );
        let marked = window.within("pane-list").find(("row", 7usize));
        assert_eq!(marked.selected(), Some(true));
        window.within("pane-list").click(("row", 2usize), cx);
        window.press("insert", cx);
        let toggled = window.within("pane-list").find(("row", 2usize));
        assert_eq!(toggled.selected(), Some(true));
    })
    .expect("window");
}

#[gpui_kit::test]
fn the_operate_page_shows_both_dialogs_and_every_job_state(cx: &mut TestAppContext) {
    let (window, _) = cx.update(|cx| {
        rusty_ui::init(Mode::Light, cx).expect("bundled fonts load");
        rusty_ui::confirm_dialog::bind_keys(cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: size(px(1280.), px(720.)),
            })),
            ..Default::default()
        };
        rusty_ui::open_window(options, cx, |window, cx| {
            cx.new(|cx| Gallery::new(Mode::Light, window, cx))
        })
        .expect("open the gallery window")
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("Operate", cx);
        for id in ["confirm-permanent", "confirm-protected"] {
            assert_eq!(window.find(id).role(), Some(Role::AlertDialog), "{id}");
            assert!(window.find(id).visible(), "{id} is not on screen");
        }
        for index in 0..4usize {
            assert!(window.find(("job", index)).visible(), "job {index}");
        }
        assert!(window.within(("job", 0usize)).try_find("cancel").is_some());
        assert!(window.within(("job", 1usize)).try_find("cancel").is_none());
        assert!(window.find("prompt-rename").visible());
    })
    .expect("window");
}

#[gpui_kit::test]
fn the_drive_pages_show_both_pills_the_menu_and_a_path_error(cx: &mut TestAppContext) {
    let (window, _) = cx.update(|cx| {
        rusty_ui::init(Mode::Light, cx).expect("bundled fonts load");
        rusty_ui::file_list::bind_keys(cx);
        rusty_ui::pane_chrome::bind_keys(cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: size(px(1280.), px(720.)),
            })),
            ..Default::default()
        };
        rusty_ui::open_window(options, cx, |window, cx| {
            cx.new(|cx| Gallery::new(Mode::Light, window, cx))
        })
        .expect("open the gallery window")
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("Drives", cx);
        for side in 0..2usize {
            assert!(window.find(("drive-pill", side)).visible(), "pill {side}");
        }
        assert_eq!(
            window.find(("drive-pill", 0usize)).label(),
            Some("Drive C: System")
        );
        let menu = window.within(("drive-menu", 0usize));
        assert!(menu.find(("drive-row", 4usize)).visible());
        assert_eq!(menu.find(("drive-row", 1usize)).selected(), Some(true));
        window.click("Path", cx);
        assert!(window.find("path-error").visible());
        assert_eq!(
            window.find(("drive-pill", 1usize)).label(),
            Some("Drive D: Work")
        );
    })
    .expect("window");
}

#[gpui_kit::test]
fn the_tokens_and_palettes_pages_render_in_both_themes(cx: &mut TestAppContext) {
    let (window, gallery) = cx.update(|cx| {
        rusty_ui::init(Mode::Light, cx).expect("bundled fonts load");
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: size(px(1280.), px(720.)),
            })),
            ..Default::default()
        };
        rusty_ui::open_window(options, cx, |window, cx| {
            cx.new(|cx| Gallery::new(Mode::Light, window, cx))
        })
        .expect("open the gallery window")
    });
    for (page, mode) in [Page::Tokens, Page::Palettes]
        .into_iter()
        .flat_map(|page| [Mode::Light, Mode::Dark].map(|mode| (page, mode)))
    {
        cx.update(|cx| {
            gallery.update(cx, |gallery, cx| {
                gallery.set_page(page, cx);
                gallery.set_mode(mode, cx);
            });
        });
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(
                window.try_find("copy").is_none(),
                "{page:?} {mode:?}: still on Parts"
            );
        })
        .expect("window");
        cx.update(|cx| assert_eq!(tokens::current(cx).mode, mode));
    }
}

#[gpui_kit::test]
fn init_registers_the_extra_grammars(cx: &mut TestAppContext) {
    cx.update(|cx| rusty_ui::init(Mode::Light, cx).expect("bundled fonts load"));
    let registry = gpui_kit::component::highlighter::LanguageRegistry::singleton();
    for name in ["xml", "powershell", "sql", "ini"] {
        let config = registry.language(name);
        assert!(config.is_some_and(|config| config.has_grammar()), "{name}");
    }
}

#[test]
fn the_page_table_follows_the_page_order() {
    for (index, info) in crate::gallery::PAGES.iter().enumerate() {
        assert_eq!(info.page as usize, index, "{}", info.label);
    }
}

#[gpui_kit::test]
fn the_sheet_pages_cover_the_passive_pane_on_each_side(cx: &mut TestAppContext) {
    let (window, gallery) = cx.update(|cx| {
        rusty_ui::init(Mode::Light, cx).expect("bundled fonts load");
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: size(px(1440.), px(720.)),
            })),
            ..Default::default()
        };
        rusty_ui::open_window(options, cx, |window, cx| {
            cx.new(|cx| Gallery::new(Mode::Light, window, cx))
        })
        .expect("open the gallery window")
    });
    for (page, is_right, is_ringed) in [
        (Page::Sheet, true, false),
        (Page::SheetLeft, false, true),
        (Page::SheetNarrow, true, false),
    ] {
        cx.update(|cx| gallery.update(cx, |gallery, cx| gallery.set_page(page, cx)));
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let sheet = window.find("preview-sheet");
            assert_eq!(sheet.label(), Some("Preview"), "{page:?}");
            assert_eq!(sheet.selected(), Some(is_ringed), "{page:?}");
            let passive = window.find(("pane", usize::from(is_right))).bounds();
            assert_eq!(sheet.bounds().size.height, passive.size.height, "{page:?}");
            let close = window.within("preview-sheet").find("side-sheet-close");
            assert!(close.visible(), "{page:?}");
        })
        .expect("window");
    }
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let sheet = window.find("preview-sheet").bounds();
        let passive = window.find(("pane", 1usize)).bounds();
        assert_eq!(sheet.left(), passive.left());
        assert_eq!(sheet.right(), passive.right());
    })
    .expect("window");
}
