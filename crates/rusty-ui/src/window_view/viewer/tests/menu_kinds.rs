use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, MouseButton, TestAppContext, point, px};
use image::ImageFormat;

use super::image::{image_view, open_first, rig};
use super::menu::{LINK, click_with, has, middle, press_key, right_click, rows, run_item};
use super::structured::open_named;
use super::{Setup, has_editor, kind, mixed, text_of, text_window};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::press;
use crate::window_view::viewer::image::fixtures::encoded;
use crate::window_view::viewer::{MenuEntry, PagedKind};

fn run_names(entries: &[MenuEntry], out: &mut Vec<&'static str>) {
    for entry in entries {
        match entry {
            MenuEntry::Run(_, action, _) => out.push(action.name()),
            MenuEntry::Submenu(_, children) => run_names(children, out),
            MenuEntry::Break => {}
        }
    }
}

fn menu_actions(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Vec<&'static str> {
    view.update(cx, |view, cx| {
        let body = view.viewer.as_ref().and_then(|viewer| viewer.body.as_ref());
        let entries = body.map(|body| body.menu(false, cx)).unwrap_or_default();
        let mut names = Vec::new();
        run_names(&entries, &mut names);
        names
    })
}

fn title(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> String {
    view.read_with(cx, |view, _| {
        view.viewer
            .as_ref()
            .map(|viewer| viewer.title.to_string())
            .unwrap_or_default()
    })
}

fn left_click(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    at: gpui_kit::Point<gpui_kit::Pixels>,
) {
    click_with(cx, window, (MouseButton::Left, at), true);
    click_with(cx, window, (MouseButton::Left, at), false);
}

fn expected(kind: &str) -> &'static [&'static str] {
    match kind {
        "editor F3" | "editor F4" => &[
            "viewer::FindSelection",
            "viewer::CopyPathLine",
            "viewer::OpenGoTo",
        ],
        "tree" => &["viewer_tree::CopyValue", "viewer_tree::CopyKey"],
        "table" => &["viewer_table::CopyColumn"],
        "markdown" => &["viewer_markdown::CopyBlock", "viewer_markdown::CopyLink"],
        "image" => &["viewer::CopyImage", "viewer::CopyFilePath"],
        _ => &[],
    }
}

#[gpui_kit::test]
fn every_menu_item_of_every_kind_names_a_registered_action(cx: &mut TestAppContext) {
    let mut names = Vec::new();
    let (window, view, _) = text_window(cx, Setup::new(b"hello\n".to_vec()));
    names.push(("editor F3", menu_actions(cx, &view)));
    press(cx, window, &view, "ctrl-e");
    names.push(("editor F4", menu_actions(cx, &view)));
    let (_, view, _) = text_window(cx, Setup::huge(b"hello\nworld\n".to_vec()));
    names.push(("paged text", menu_actions(cx, &view)));
    let (_, view, _) = text_window(cx, Setup::new(mixed(4096)));
    names.push(("paged hex", menu_actions(cx, &view)));
    let (window, view, _) = open_named(
        cx,
        "links.md",
        Setup::new(b"[one](https://example.com/1)\n".to_vec()),
    );
    press(cx, window, &view, "tab");
    names.push(("markdown", menu_actions(cx, &view)));
    let (_, view, _) = open_named(cx, "shop.json", Setup::new(br#"{"a": 1}"#.to_vec()));
    names.push(("tree", menu_actions(cx, &view)));
    let (_, view, _) = open_named(cx, "fruit.csv", Setup::new(b"name,qty\nPear,10\n".to_vec()));
    names.push(("table", menu_actions(cx, &view)));
    let image = rig(cx, &[], &[("a.txt", encoded(ImageFormat::Png, 300, 200))]);
    open_first(cx, &image);
    names.push(("image", menu_actions(cx, &image.1)));
    for (kind, actions) in &names {
        for wanted in expected(kind) {
            assert!(
                actions.contains(wanted),
                "{kind} offers {wanted}: {actions:?}"
            );
        }
    }
    for (kind, actions) in names {
        assert!(!actions.is_empty(), "{kind} has items");
        for name in actions {
            let built = cx.update(|cx| cx.build_action(name, None));
            assert!(built.is_ok(), "{kind}: {name} is not a registered action");
        }
    }
}

#[gpui_kit::test]
fn the_paged_text_menu_runs_open_as_hex(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::huge(b"hello\nworld\n".to_vec()));
    assert_eq!(kind(cx, &view), Some(PagedKind::Text));
    let at = middle(cx, window);
    right_click(cx, window, at);
    assert!(has(&rows(cx, window), "Go to line"));
    run_item(cx, window, "open as hex");
    assert_eq!(kind(cx, &view), Some(PagedKind::Hex));
}

#[gpui_kit::test]
fn the_image_menu_runs_100_percent(cx: &mut TestAppContext) {
    let image = rig(cx, &[], &[("a.txt", encoded(ImageFormat::Png, 3000, 2000))]);
    open_first(cx, &image);
    let window = image.0;
    assert_ne!(text_of(cx, window, "image-zoom"), "100 %");
    let at = middle(cx, window);
    right_click(cx, window, at);
    assert_eq!(
        rows(cx, window),
        ["Copy image", "Copy path", "-", "Fit", "100 %"]
    );
    run_item(cx, window, "100");
    assert_eq!(text_of(cx, window, "image-zoom"), "100 %");
    assert!(image_view(cx, &image.1).is_some());
}

#[gpui_kit::test]
fn open_link_from_the_menu_opens_the_picked_link(cx: &mut TestAppContext) {
    let source = b"[one](https://example.com/1)\n".to_vec();
    let (window, view, _) = open_named(cx, "links.md", Setup::new(source));
    press(cx, window, &view, "tab");
    press_key(cx, window, "menu");
    run_item(cx, window, "open link");
    assert_eq!(
        text_of(cx, window, "markdown-status"),
        "Opened https://example.com/1"
    );
}

#[gpui_kit::test]
fn a_left_clicked_link_is_not_offered_by_a_later_right_click_elsewhere(cx: &mut TestAppContext) {
    let filler = "Plain words that are not a link.\n\n".repeat(80);
    let source = format!("[{LINK} {LINK} {LINK}](https://example.com)\n\n{filler}");
    let (window, _, _) = open_named(cx, "notes.md", Setup::new(source.into_bytes()));
    let block = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.find(("md-block", 0usize)).bounds()
        })
        .expect("window");
    left_click(cx, window, block.center());
    assert!(text_of(cx, window, "markdown-link").starts_with("Enter opens"));
    let body = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.find("markdown-body").bounds()
        })
        .expect("window");
    let scrollbar = point(body.right() - px(4.), body.center().y);
    right_click(cx, window, scrollbar);
    assert!(!has(&rows(cx, window), "Open link"), "on the scrollbar");
    press_key(cx, window, "escape");
    left_click(cx, window, block.center());
    right_click(cx, window, point(block.center().x, body.bottom() - px(60.)));
    assert!(!has(&rows(cx, window), "Open link"), "on plain text");
}

#[gpui_kit::test]
fn menu_and_shift_f10_open_the_tree_and_table_menus(cx: &mut TestAppContext) {
    let (window, _, _) = open_named(cx, "shop.json", Setup::new(br#"{"a": 1}"#.to_vec()));
    press_key(cx, window, "menu");
    assert!(has(&rows(cx, window), "Copy path"));
    press_key(cx, window, "escape");
    press_key(cx, window, "shift-f10");
    assert!(has(&rows(cx, window), "Fold all"));
    let (window, _, _) = open_named(cx, "fruit.csv", Setup::new(b"name,qty\nPear,10\n".to_vec()));
    press_key(cx, window, "menu");
    assert!(has(&rows(cx, window), "Copy cell"));
    press_key(cx, window, "escape");
    press_key(cx, window, "shift-f10");
    assert!(has(&rows(cx, window), "Sort by this column"));
}

#[gpui_kit::test]
fn typing_n_p_and_1_in_the_filter_leaves_the_viewer_alone(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(mixed(4096)));
    let before = title(cx, &view);
    assert_eq!(kind(cx, &view), Some(PagedKind::Hex));
    press_key(cx, window, "menu");
    for key in ["n", "p", "1"] {
        press_key(cx, window, key);
    }
    assert!(!rows(cx, window).is_empty(), "the menu stays open");
    assert_eq!(title(cx, &view), before);
    assert_eq!(kind(cx, &view), Some(PagedKind::Hex));
    assert!(!has_editor(cx, &view));
}
