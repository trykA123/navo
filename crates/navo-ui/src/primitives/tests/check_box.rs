use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    Context, IntoElement, ParentElement, Render, Role, Styled, TestAppContext, Window, div,
};

use super::open;
use crate::primitives::CheckBox;

#[derive(Default)]
struct Form {
    on: bool,
    flips: usize,
}

impl Render for Form {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .gap_2()
            .child(
                CheckBox::new("accept", "I &accept")
                    .checked(self.on)
                    .on_toggle(cx.listener(|this, _, _, cx| {
                        this.on = !this.on;
                        this.flips += 1;
                        cx.notify();
                    })),
            )
            .child(
                CheckBox::new("locked", "Locked")
                    .disabled(true)
                    .on_toggle(cx.listener(|this, _, _, cx| {
                        this.flips += 100;
                        cx.notify();
                    })),
            )
    }
}

#[gpui_kit::test]
fn a_check_box_reports_its_state_and_toggles_on_click(cx: &mut TestAppContext) {
    let (window, view) = open(cx, |_, _| Form::default());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let accept = window.find("accept");
        assert_eq!(accept.role(), Some(Role::CheckBox));
        assert_eq!(accept.label(), Some("I accept"));
        assert_eq!(accept.checked(), Some(false));
        window.click("accept", cx);
        window.render_frame(cx);
        assert_eq!(window.find("accept").checked(), Some(true));
    })
    .expect("window");
    assert_eq!(view.read_with(cx, |view, _| view.flips), 1);
}

#[gpui_kit::test]
fn a_disabled_check_box_ignores_clicks(cx: &mut TestAppContext) {
    let (window, view) = open(cx, |_, _| Form::default());
    cx.update_window(window, |_, window, cx| window.click("locked", cx))
        .expect("window");
    assert_eq!(view.read_with(cx, |view, _| view.flips), 0);
}
