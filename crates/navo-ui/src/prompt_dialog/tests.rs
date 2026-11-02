use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Context, Entity, IntoElement, ParentElement, Render,
    SharedString, Styled, TestAppContext, Window, div,
};

use super::{PromptDialog, PromptProps};
use crate::test_window::open;

const DIALOG: &str = "prompt";

fn rename() -> PromptProps {
    PromptProps {
        title: "Rename".into(),
        label: "New name".into(),
        value: "report.txt".into(),
        select: 0..6,
        confirm_label: "Rename".into(),
        error: None,
    }
}

#[derive(Default)]
struct Harness {
    props: Option<PromptProps>,
    submitted: Vec<SharedString>,
    cancelled: usize,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (submit, cancel) = (cx.entity(), cx.entity());
        let props = self.props.clone().unwrap_or_else(rename);
        let dialog = PromptDialog::new(DIALOG, props)
            .on_submit(move |value, _, cx| {
                let value = value.clone();
                submit.update(cx, |this, _| this.submitted.push(value));
            })
            .on_cancel(move |_, cx| cancel.update(cx, |this, _| this.cancelled += 1));
        div().size_full().child(dialog)
    }
}

fn open_prompt(cx: &mut TestAppContext, props: PromptProps) -> (AnyWindowHandle, Entity<Harness>) {
    let (window, harness) = open(cx, |_, _| Harness {
        props: Some(props),
        ..Harness::default()
    });
    cx.update(crate::prompt_dialog::bind_keys);
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    cx.run_until_parked();
    (window, harness)
}

fn press(window: AnyWindowHandle, key: &str, cx: &mut TestAppContext) {
    cx.update_window(window, |_, window, cx| window.press(key, cx))
        .expect("window");
    cx.run_until_parked();
}

fn type_text(window: AnyWindowHandle, text: &str, cx: &mut TestAppContext) {
    cx.update_window(window, |_, window, cx| window.input(text, cx))
        .expect("window");
    cx.run_until_parked();
}

fn submitted(harness: &Entity<Harness>, cx: &mut TestAppContext) -> Vec<SharedString> {
    harness.read_with(cx, |harness, _| harness.submitted.clone())
}

#[gpui_kit::test]
fn the_field_starts_focused_with_the_stem_selected(cx: &mut TestAppContext) {
    let (window, harness) = open_prompt(cx, rename());
    cx.update_window(window, |_, window, _| {
        assert_eq!(window.within(DIALOG).find("field").focused(), Some(true));
    })
    .expect("window");
    type_text(window, "summary", cx);
    press(window, "enter", cx);
    assert_eq!(submitted(&harness, cx), ["summary.txt"]);
}

#[gpui_kit::test]
fn escape_cancels(cx: &mut TestAppContext) {
    let (window, harness) = open_prompt(cx, rename());
    press(window, "escape", cx);
    assert_eq!(harness.read_with(cx, |h, _| h.cancelled), 1);
    assert!(submitted(&harness, cx).is_empty());
}

#[gpui_kit::test]
fn the_confirm_button_submits(cx: &mut TestAppContext) {
    let (window, harness) = open_prompt(cx, rename());
    cx.update_window(window, |_, window, cx| {
        window.within(DIALOG).click("confirm-button", cx);
    })
    .expect("window");
    cx.run_until_parked();
    assert_eq!(submitted(&harness, cx), ["report.txt"]);
}

#[gpui_kit::test]
fn an_error_is_shown_under_the_field(cx: &mut TestAppContext) {
    let props = PromptProps {
        error: Some("A name cannot contain ?".into()),
        ..rename()
    };
    let (window, _) = open_prompt(cx, props);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let error = window.within(DIALOG).find("error");
        assert_eq!(error.label(), Some("A name cannot contain ?"));
    })
    .expect("window");
}

#[gpui_kit::test]
fn escape_after_typing_cancels(cx: &mut TestAppContext) {
    let (window, harness) = open_prompt(cx, rename());
    type_text(window, "docs", cx);
    press(window, "escape", cx);
    assert_eq!(harness.read_with(cx, |h, _| h.cancelled), 1);
}

#[gpui_kit::test]
fn escape_after_enter_cancels(cx: &mut TestAppContext) {
    let (window, harness) = open_prompt(cx, rename());
    press(window, "enter", cx);
    press(window, "escape", cx);
    assert_eq!(harness.read_with(cx, |h, _| h.cancelled), 1);
}

fn focused(window: AnyWindowHandle, cx: &mut TestAppContext) -> Vec<&'static str> {
    cx.update_window(window, |_, window, _| {
        ["field", "confirm", "cancel"]
            .into_iter()
            .filter(|id| {
                window
                    .within(DIALOG)
                    .try_find(*id)
                    .and_then(|found| found.focused())
                    == Some(true)
            })
            .collect()
    })
    .expect("window")
}

#[gpui_kit::test]
fn tab_order_is_field_confirm_cancel_and_wraps(cx: &mut TestAppContext) {
    let (window, _) = open_prompt(cx, rename());
    for expected in ["confirm", "cancel", "field", "confirm"] {
        press(window, "tab", cx);
        assert_eq!(focused(window, cx), [expected]);
    }
    press(window, "shift-tab", cx);
    assert_eq!(focused(window, cx), ["field"]);
    press(window, "shift-tab", cx);
    assert_eq!(focused(window, cx), ["cancel"]);
}

#[gpui_kit::test]
fn the_arrow_keys_move_between_the_buttons(cx: &mut TestAppContext) {
    let (window, _) = open_prompt(cx, rename());
    press(window, "tab", cx);
    press(window, "right", cx);
    assert_eq!(focused(window, cx), ["cancel"]);
    press(window, "right", cx);
    assert_eq!(focused(window, cx), ["confirm"]);
    press(window, "left", cx);
    assert_eq!(focused(window, cx), ["cancel"]);
}

#[gpui_kit::test]
fn enter_and_space_activate_the_focused_button(cx: &mut TestAppContext) {
    let (window, harness) = open_prompt(cx, rename());
    press(window, "tab", cx);
    press(window, "space", cx);
    assert_eq!(submitted(&harness, cx), ["report.txt"]);
    press(window, "tab", cx);
    press(window, "enter", cx);
    assert_eq!(harness.read_with(cx, |h, _| h.cancelled), 1);
    press(window, "space", cx);
    assert_eq!(harness.read_with(cx, |h, _| h.cancelled), 2);
}

#[gpui_kit::test]
fn space_and_arrows_still_edit_the_field(cx: &mut TestAppContext) {
    let (window, harness) = open_prompt(cx, rename());
    press(window, "left", cx);
    press(window, "right", cx);
    press(window, "space", cx);
    assert_eq!(focused(window, cx), ["field"]);
    assert!(submitted(&harness, cx).is_empty());
}
