use std::time::Duration;

use gpui_kit::AppContext as _;
use gpui_kit::test::{TestAppContextExt, TestSupportExt, TestWindowExt};
use gpui_kit::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, TestAppContext, Window, div, px,
};

use super::open;
use crate::primitives::Tooltip;

struct Trigger;

impl Render for Trigger {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().p_4().child(
            div()
                .id("copy")
                .test_support()
                .size(px(40.))
                .tooltip(|window, cx| Tooltip::new("Copy path").keys("ctrl-c").build(window, cx)),
        )
    }
}

#[gpui_kit::test]
async fn hovering_shows_the_tooltip(cx: &mut TestAppContext) {
    let (window, _) = open(cx, |_, _| Trigger);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("tooltip").is_none());
        window.hover("copy", cx);
    })
    .expect("window");
    cx.wait_for(window, Duration::from_secs(2), |window, _| {
        window
            .try_find("tooltip")
            .is_some_and(|tooltip| tooltip.label() == Some("Copy path"))
    })
    .await;
}
