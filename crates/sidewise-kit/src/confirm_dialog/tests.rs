use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Context, Entity, IntoElement, ParentElement, Render, Role,
    SharedString, Styled, TestAppContext, Window, div,
};

use super::{Choice, ConfirmDialog, ConfirmProps};
use crate::test_window::open;

pub(super) const DIALOG: &str = "dialog";

pub(super) fn props(lines: usize, more: u64) -> ConfirmProps {
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
        mode: None,
        choice: None,
    }
}

#[derive(Default)]
pub(super) struct Counts {
    pub(super) confirmed: usize,
    pub(super) cancelled: usize,
    pub(super) toggled: usize,
    pub(super) switched: usize,
}

pub(super) struct Harness {
    pub(super) props: ConfirmProps,
    pub(super) switch: Option<Choice>,
    pub(super) counts: Counts,
}

impl Harness {
    /// A harness for `props` with no switch row and zero counts.
    pub(super) fn new(props: ConfirmProps) -> Self {
        Self {
            props,
            switch: None,
            counts: Counts::default(),
        }
    }
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let confirm = cx.entity();
        let cancel = cx.entity();
        let toggle = cx.entity();
        let switch = cx.entity();
        let dialog = ConfirmDialog::new(DIALOG, self.props.clone())
            .on_confirm(move |_, cx| confirm.update(cx, |this, _| this.counts.confirmed += 1))
            .on_cancel(move |_, cx| cancel.update(cx, |this, _| this.counts.cancelled += 1))
            .on_toggle(move |_, cx| toggle.update(cx, |this, _| this.counts.toggled += 1));
        let dialog = match self.switch.clone() {
            Some(choice) => dialog.switch(choice, move |_, cx| {
                switch.update(cx, |this, _| this.counts.switched += 1);
            }),
            None => dialog,
        };
        div().size_full().child(dialog)
    }
}

fn open_dialog(cx: &mut TestAppContext, props: ConfirmProps) -> (AnyWindowHandle, Entity<Harness>) {
    open_harness(cx, Harness::new(props))
}

/// Opens a window that draws `harness` and renders its first frame.
pub(super) fn open_harness(
    cx: &mut TestAppContext,
    harness: Harness,
) -> (AnyWindowHandle, Entity<Harness>) {
    let (window, harness) = open(cx, |_, _| harness);
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    cx.run_until_parked();
    (window, harness)
}

pub(super) fn press(window: AnyWindowHandle, keys: &[&str], cx: &mut TestAppContext) {
    for key in keys {
        cx.update_window(window, |_, window, cx| window.press(key, cx))
            .expect("window");
        cx.run_until_parked();
    }
}

pub(super) fn counts(harness: &Entity<Harness>, cx: &mut TestAppContext) -> (usize, usize) {
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
fn a_long_path_is_cut_in_the_middle_but_read_out_in_full(cx: &mut TestAppContext) {
    let long = r"C:\Users\admin\Documents\clients\acme\project\crates\core\src\main.rs";
    let mut shown = props(1, 0);
    shown.lines = vec![long.into()];
    let (window, _) = open_dialog(cx, shown);
    assert_eq!(label(window, ("line", 0usize), cx).as_deref(), Some(long));
    let line = super::elide::middle(long, super::LINE_CHARS);
    assert!(line.starts_with(r"C:\Users\"), "{line}");
    assert!(line.ends_with(r"\src\main.rs"), "{line}");
    assert!(line.contains('…'), "{line}");
}

#[gpui_kit::test]
fn a_short_list_has_no_more_line(cx: &mut TestAppContext) {
    let (window, _) = open_dialog(cx, props(2, 0));
    assert!(label(window, ("line", 1usize), cx).is_some());
    assert_eq!(label(window, "more", cx), None);
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

pub(super) fn typed() -> ConfirmProps {
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

pub(super) fn type_text(window: AnyWindowHandle, text: &str, cx: &mut TestAppContext) {
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

#[test]
fn hidden_counts_the_cut_lines_and_the_rest() {
    assert_eq!(props(7, 1).hidden(), 3);
    assert_eq!(props(5, 0).hidden(), 0);
    assert_eq!(props(2, 4).hidden(), 4);
}

pub(super) fn focused(window: AnyWindowHandle, cx: &mut TestAppContext) -> Vec<&'static str> {
    cx.update_window(window, |_, window, _| {
        ["typed", "choice", "switch", "confirm", "cancel"]
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
fn a_safe_dialog_opens_with_confirm_focused(cx: &mut TestAppContext) {
    let mut safe = props(1, 0);
    safe.danger = false;
    let (window, _) = open_dialog(cx, safe);
    assert_eq!(focused(window, cx), ["confirm"]);
}

#[gpui_kit::test]
fn a_danger_dialog_opens_with_cancel_focused(cx: &mut TestAppContext) {
    let (window, _) = open_dialog(cx, props(1, 0));
    assert_eq!(focused(window, cx), ["cancel"]);
}

#[gpui_kit::test]
fn focus_stays_inside_the_dialog(cx: &mut TestAppContext) {
    let (window, _) = open_dialog(cx, props(1, 0));
    for _ in 0..5 {
        press(window, &["tab"], cx);
        assert_eq!(focused(window, cx).len(), 1);
    }
}

#[gpui_kit::test]
fn a_typed_dialog_tabs_field_confirm_cancel_and_wraps(cx: &mut TestAppContext) {
    let (window, _) = open_dialog(cx, typed());
    type_text(window, "DELETE", cx);
    for expected in ["confirm", "cancel", "typed", "confirm"] {
        press(window, &["tab"], cx);
        assert_eq!(focused(window, cx), [expected]);
    }
    press(window, &["shift-tab", "shift-tab"], cx);
    assert_eq!(focused(window, cx), ["cancel"]);
}

#[gpui_kit::test]
fn space_and_arrows_still_type_in_the_field(cx: &mut TestAppContext) {
    let (window, harness) = open_dialog(cx, typed());
    press(window, &["left", "right", "space"], cx);
    assert_eq!(focused(window, cx), ["typed"]);
    assert_eq!(counts(&harness, cx), (0, 0));
}

#[gpui_kit::test]
fn a_copy_dialog_shows_its_mode_as_a_small_label(cx: &mut TestAppContext) {
    for mode in ["Auto", "Standard"] {
        let mut copy = props(1, 0);
        copy.mode = Some(mode.into());
        let (window, _) = open_dialog(cx, copy);
        assert_eq!(label(window, "mode", cx).as_deref(), Some(mode));
    }
}

#[gpui_kit::test]
fn a_dialog_without_a_mode_has_no_mode_label(cx: &mut TestAppContext) {
    let (window, _) = open_dialog(cx, props(1, 0));
    assert_eq!(label(window, "mode", cx), None);
}

pub(super) fn with_choice() -> ConfirmProps {
    let mut safe = props(1, 0);
    safe.danger = false;
    safe.choice = Some(Choice {
        label: "Keep folder structure".into(),
        on: true,
    });
    safe
}

pub(super) fn toggled(harness: &Entity<Harness>, cx: &mut TestAppContext) -> usize {
    harness.read_with(cx, |harness, _| harness.counts.toggled)
}

#[gpui_kit::test]
fn a_click_on_the_choice_toggles_it(cx: &mut TestAppContext) {
    let (window, harness) = open_dialog(cx, with_choice());
    cx.update_window(window, |_, window, cx| {
        window.within(DIALOG).click("choice", cx);
    })
    .expect("window");
    cx.run_until_parked();
    assert_eq!(toggled(&harness, cx), 1);
}

#[gpui_kit::test]
fn without_a_choice_tab_skips_straight_to_the_buttons(cx: &mut TestAppContext) {
    let mut safe = props(1, 0);
    safe.danger = false;
    let (window, _) = open_dialog(cx, safe);
    press(window, &["shift-tab"], cx);
    assert_eq!(focused(window, cx), ["cancel"]);
}

/// A safe dialog with the choice and a "Verify after copy" switch that is off.
pub(super) fn with_switch() -> Harness {
    let mut harness = Harness::new(with_choice());
    harness.switch = Some(Choice {
        label: "Verify after copy".into(),
        on: false,
    });
    harness
}

pub(super) fn switched(harness: &Entity<Harness>, cx: &mut TestAppContext) -> usize {
    harness.read_with(cx, |harness, _| harness.counts.switched)
}

#[gpui_kit::test]
fn a_click_on_the_switch_calls_its_handler_only(cx: &mut TestAppContext) {
    let (window, harness) = open_harness(cx, with_switch());
    cx.update_window(window, |_, window, cx| {
        window.within(DIALOG).click("switch", cx);
    })
    .expect("window");
    cx.run_until_parked();
    assert_eq!(switched(&harness, cx), 1);
    assert_eq!(toggled(&harness, cx), 0);
    assert_eq!(counts(&harness, cx), (0, 0));
}

#[gpui_kit::test]
fn the_switch_row_is_absent_when_unset(cx: &mut TestAppContext) {
    let (window, _) = open_dialog(cx, with_choice());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.within(DIALOG).try_find("choice").is_some());
        assert!(window.within(DIALOG).try_find("switch").is_none());
    })
    .expect("window");
}
