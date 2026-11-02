use gpui_kit::AppContext as _;
use gpui_kit::base::TestSupportExt;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, Context, FocusHandle, InteractiveElement, IntoElement, ParentElement, Render,
    Styled, TestAppContext, Window, div, px,
};

use super::open;
use crate::primitives::{Tab, Tabs};

const COUNT: usize = 10;

struct Pane {
    selected: usize,
    list: FocusHandle,
}

impl Render for Pane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = (0..COUNT).map(|index| Tab::new(format!("calibration-export-{index}")));
        div()
            .w(px(400.))
            .child(
                Tabs::new("tabs", tabs)
                    .selected(self.selected)
                    .on_select(cx.listener(|this, index: &usize, _, cx| {
                        this.selected = *index;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("list")
                    .test_support()
                    .track_focus(&self.list)
                    .size(px(100.)),
            )
    }
}

fn open_pane(cx: &mut TestAppContext) -> (AnyWindowHandle, gpui_kit::Entity<Pane>) {
    cx.update(crate::primitives::popup_menu::bind_keys);
    let (window, view) = open(cx, |_, cx| Pane {
        selected: 0,
        list: cx.focus_handle(),
    });
    cx.update_window(window, |_, window, cx| {
        let list = view.read(cx).list.clone();
        window.focus(&list, cx);
        window.render_frame(cx);
    })
    .expect("window");
    (window, view)
}

fn first_hidden(window: &mut Window) -> usize {
    let tabs = window.within("tabs");
    (0..COUNT)
        .find(|index| tabs.try_find(*index).is_none())
        .expect("a hidden tab")
}

fn is_open(window: &mut Window) -> bool {
    window.within("tabs").try_find("tab-more-menu").is_some()
}

#[gpui_kit::test]
fn the_more_menu_opens_with_focus_on_its_first_row(cx: &mut TestAppContext) {
    let (window, _) = open_pane(cx);
    cx.update_window(window, |_, window, cx| {
        let first = first_hidden(window);
        window.within("tabs").click("tab-more", cx);
        let tabs = window.within("tabs");
        assert_eq!(tabs.find("tab-more-menu").focused(), Some(true));
        assert_eq!(tabs.find(("tab-more-row", first)).selected(), Some(true));
        assert_eq!(window.find("list").focused(), Some(false));
    })
    .expect("window");
}

#[gpui_kit::test]
fn arrows_move_and_enter_opens_the_hidden_tab(cx: &mut TestAppContext) {
    let (window, view) = open_pane(cx);
    let first = cx
        .update_window(window, |_, window, cx| {
            let first = first_hidden(window);
            window.within("tabs").click("tab-more", cx);
            window.press("down", cx);
            window.press("down", cx);
            window.press("up", cx);
            let tabs = window.within("tabs");
            assert_eq!(
                tabs.find(("tab-more-row", first + 1)).selected(),
                Some(true)
            );
            assert_eq!(tabs.find(("tab-more-row", first)).selected(), Some(false));
            window.press("enter", cx);
            assert!(!is_open(window));
            assert_eq!(window.find("list").focused(), Some(true));
            first
        })
        .expect("window");
    assert_eq!(view.read_with(cx, |view, _| view.selected), first + 1);
}

#[gpui_kit::test]
fn escape_closes_the_more_menu_and_returns_focus(cx: &mut TestAppContext) {
    let (window, view) = open_pane(cx);
    cx.update_window(window, |_, window, cx| {
        window.within("tabs").click("tab-more", cx);
        window.press("escape", cx);
        assert!(!is_open(window));
        assert_eq!(window.find("list").focused(), Some(true));
    })
    .expect("window");
    assert_eq!(view.read_with(cx, |view, _| view.selected), 0);
}

#[gpui_kit::test]
fn clicking_the_count_again_closes_the_menu(cx: &mut TestAppContext) {
    let (window, _) = open_pane(cx);
    cx.update_window(window, |_, window, cx| {
        window.within("tabs").click("tab-more", cx);
        assert!(is_open(window));
        window.within("tabs").click("tab-more", cx);
        window.render_frame(cx);
        assert!(!is_open(window));
        assert_eq!(
            window.within("tabs").find("tab-more").expanded(),
            Some(false)
        );
        assert_eq!(window.find("list").focused(), Some(true));
        window.within("tabs").click("tab-more", cx);
        assert!(is_open(window));
    })
    .expect("window");
}
