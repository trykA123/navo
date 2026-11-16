use gpui_kit::AppContext as _;
use gpui_kit::assets::IconName;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    Context, IntoElement, ParentElement, Render, Role, Styled, TestAppContext, Window, div,
};

use super::open;
use crate::primitives::{Button, ButtonVariant, IconButton};

#[derive(Default)]
struct Clicks {
    count: usize,
}

impl Render for Clicks {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .gap_2()
            .child(
                Button::new("save", "Save")
                    .variant(ButtonVariant::Primary)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.count += 1;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("locked", "Locked")
                    .disabled(true)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.count += 100;
                        cx.notify();
                    })),
            )
            .child(
                IconButton::new("theme", IconName::Moon, "Dark theme").on_click(cx.listener(
                    |this, _, _, cx| {
                        this.count += 10;
                        cx.notify();
                    },
                )),
            )
    }
}

#[gpui_kit::test]
fn a_button_click_fires_its_handler(cx: &mut TestAppContext) {
    let (window, view) = open(cx, |_, _| Clicks::default());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("save").role(), Some(Role::Button));
        window.click("save", cx);
    })
    .expect("window");
    assert_eq!(view.read_with(cx, |view, _| view.count), 1);
}

#[gpui_kit::test]
fn a_disabled_button_ignores_clicks(cx: &mut TestAppContext) {
    let (window, view) = open(cx, |_, _| Clicks::default());
    cx.update_window(window, |_, window, cx| window.click("locked", cx))
        .expect("window");
    assert_eq!(view.read_with(cx, |view, _| view.count), 0);
}

#[gpui_kit::test]
fn an_icon_button_is_named_and_fires(cx: &mut TestAppContext) {
    let (window, view) = open(cx, |_, _| Clicks::default());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("theme").label(), Some("Dark theme"));
        window.click("theme", cx);
    })
    .expect("window");
    assert_eq!(view.read_with(cx, |view, _| view.count), 10);
}
