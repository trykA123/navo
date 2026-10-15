use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Context, Entity, IntoElement, ParentElement, Render, Role,
    SharedString, Styled, TestAppContext, Window, div,
};

use super::{ConfirmDialog, ConfirmProps};
use crate::test_window::open;

const DIALOG: &str = "dialog";

fn props(lines: usize, more: u64) -> ConfirmProps {
    ConfirmProps {
        title: "Delete 8 items permanently?".into(),
        lines: (0..lines)
            .map(|index| SharedString::from(format!(r"D:\work\file-{index}.txt")))
            .collect(),
        more,
        summary: "7 files, 1 folder · 12,345 B".into(),
        confirm_label: "Delete permanently".into(),
        danger: true,
        typed: None,
    }
}

#[derive(Default)]
struct Counts {
    confirmed: usize,
    cancelled: usize,
}

struct Harness {
    props: ConfirmProps,
    counts: Counts,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let confirm = cx.entity();
        let cancel = cx.entity();
        let dialog = ConfirmDialog::new(DIALOG, self.props.clone())
            .on_confirm(move |_, cx| confirm.update(cx, |this, _| this.counts.confirmed += 1))
            .on_cancel(move |_, cx| cancel.update(cx, |this, _| this.counts.cancelled += 1));
        div().size_full().child(dialog)
    }
}

fn open_dialog(cx: &mut TestAppContext, props: ConfirmProps) -> (AnyWindowHandle, Entity<Harness>) {
    let (window, harness) = open(cx, |_, _| Harness {
        props,
        counts: Counts::default(),
    });
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    cx.run_until_parked();
    (window, harness)
}

fn press(window: AnyWindowHandle, keys: &[&str], cx: &mut TestAppContext) {
    for key in keys {
        cx.update_window(window, |_, window, cx| window.press(key, cx))
            .expect("window");
        cx.run_until_parked();
    }
}

fn counts(harness: &Entity<Harness>, cx: &mut TestAppContext) -> (usize, usize) {
    harness.read_with(cx, |harness, _| {
        (harness.counts.confirmed, harness.counts.cancelled)
    })
}

fn label(
    window: AnyWindowHandle,
    id: impl Into<gpui_kit::ElementId>,
    cx: &mut TestAppContext,
) -> Option<String> {
    let id = id.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within(DIALOG)
            .try_find(id)
            .and_then(|found| found.label().map(str::to_owned))
    })
    .expect("window")
}

#[gpui_kit::test]
fn it_shows_the_title_five_paths_the_rest_as_a_count_and_the_summary(cx: &mut TestAppContext) {
    let (window, _) = open_dialog(cx, props(7, 1));
    assert_eq!(
        label(window, "title", cx).as_deref(),
        Some("Delete 8 items permanently?")
    );
    for index in 0..5usize {
        let path = format!(r"D:\work\file-{index}.txt");
        assert_eq!(label(window, ("line", index), cx), Some(path));
    }
    assert_eq!(label(window, ("line", 5usize), cx), None);
    assert_eq!(label(window, "more", cx).as_deref(), Some("and 3 more"));
    assert_eq!(
        label(window, "summary", cx).as_deref(),
        Some("7 files, 1 folder · 12,345 B")
    );
    cx.update_window(window, |_, window, _| {
        assert_eq!(window.find(DIALOG).role(), Some(Role::AlertDialog));
    })
    .expect("window");
}

#[gpui_kit::test]
fn a_short_list_has_no_more_line(cx: &mut TestAppContext) {
    let (window, _) = open_dialog(cx, props(2, 0));
    assert!(label(window, ("line", 1usize), cx).is_some());
    assert_eq!(label(window, "more", cx), None);
}

#[gpui_kit::test]
fn escape_cancels(cx: &mut TestAppContext) {
    let (window, harness) = open_dialog(cx, props(1, 0));
    press(window, &["escape"], cx);
    assert_eq!(counts(&harness, cx), (0, 1));
}

#[gpui_kit::test]
fn with_danger_enter_right_away_cancels(cx: &mut TestAppContext) {
    let (window, harness) = open_dialog(cx, props(1, 0));
    cx.update_window(window, |_, window, _| {
        assert_eq!(window.within(DIALOG).find("cancel").focused(), Some(true));
        assert_eq!(window.within(DIALOG).find("confirm").focused(), Some(false));
    })
    .expect("window");
    press(window, &["enter"], cx);
    assert_eq!(counts(&harness, cx), (0, 1));
}

#[gpui_kit::test]
fn with_danger_tab_then_enter_confirms(cx: &mut TestAppContext) {
    let (window, harness) = open_dialog(cx, props(1, 0));
    press(window, &["tab", "enter"], cx);
    assert_eq!(counts(&harness, cx), (1, 0));
}

#[gpui_kit::test]
fn tab_and_shift_tab_move_between_the_buttons(cx: &mut TestAppContext) {
    let (window, harness) = open_dialog(cx, props(1, 0));
    press(
        window,
        &["tab", "tab", "shift-tab", "shift-tab", "enter"],
        cx,
    );
    assert_eq!(counts(&harness, cx), (0, 1));
}

#[gpui_kit::test]
fn without_danger_enter_confirms(cx: &mut TestAppContext) {
    let mut safe = props(1, 0);
    safe.danger = false;
    let (window, harness) = open_dialog(cx, safe);
    press(window, &["enter"], cx);
    assert_eq!(counts(&harness, cx), (1, 0));
}

#[gpui_kit::test]
fn clicks_on_the_buttons_confirm_and_cancel(cx: &mut TestAppContext) {
    let (window, harness) = open_dialog(cx, props(1, 0));
    cx.update_window(window, |_, window, cx| {
        window.within(DIALOG).click("confirm-button", cx);
        window.within(DIALOG).click("cancel-button", cx);
    })
    .expect("window");
    assert_eq!(counts(&harness, cx), (1, 1));
}

fn typed() -> ConfirmProps {
    let mut protected = props(1, 0);
    protected.typed = Some("DELETE".into());
    protected
}

fn click_confirm(window: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(window, |_, window, cx| {
        window.within(DIALOG).click("confirm-button", cx);
    })
    .expect("window");
    cx.run_until_parked();
}

fn type_text(window: AnyWindowHandle, text: &str, cx: &mut TestAppContext) {
    cx.update_window(window, |_, window, cx| window.input(text, cx))
        .expect("window");
    cx.run_until_parked();
}

#[gpui_kit::test]
fn typed_confirmation_starts_in_the_field_with_confirm_disabled(cx: &mut TestAppContext) {
    let (window, harness) = open_dialog(cx, typed());
    cx.update_window(window, |_, window, _| {
        assert_eq!(window.within(DIALOG).find("typed").focused(), Some(true));
    })
    .expect("window");
    click_confirm(window, cx);
    press(window, &["enter", "tab", "enter"], cx);
    assert_eq!(counts(&harness, cx), (0, 1));
}

#[gpui_kit::test]
fn the_typed_word_is_case_sensitive(cx: &mut TestAppContext) {
    let (window, harness) = open_dialog(cx, typed());
    type_text(window, "delete", cx);
    click_confirm(window, cx);
    assert_eq!(counts(&harness, cx), (0, 0));
}

#[gpui_kit::test]
fn the_exact_word_enables_confirm(cx: &mut TestAppContext) {
    let (window, harness) = open_dialog(cx, typed());
    type_text(window, "DELETE", cx);
    press(window, &["enter"], cx);
    assert_eq!(counts(&harness, cx), (0, 0));
    press(window, &["tab", "enter"], cx);
    assert_eq!(counts(&harness, cx), (1, 0));
    click_confirm(window, cx);
    assert_eq!(counts(&harness, cx), (2, 0));
}

#[gpui_kit::test]
fn escape_in_the_field_cancels(cx: &mut TestAppContext) {
    let (window, harness) = open_dialog(cx, typed());
    type_text(window, "DEL", cx);
    press(window, &["escape"], cx);
    assert_eq!(counts(&harness, cx), (0, 1));
}

#[test]
fn hidden_counts_the_cut_lines_and_the_rest() {
    assert_eq!(props(7, 1).hidden(), 3);
    assert_eq!(props(5, 0).hidden(), 0);
    assert_eq!(props(2, 4).hidden(), 4);
}
