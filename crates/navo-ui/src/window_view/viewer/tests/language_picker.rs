//! Language picker tests: three ways to open it, the rows, choosing, the unsaved guard and Hex.
//! They drive the viewer by keys, the View menu and a click on the header button.

use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext, Entity, Pixels, TestAppContext, px};
use navo_fs::FakeFs;

use super::menu::{press_key, run_item};
use super::search::with_paged;
use super::structured::fake_with;
use super::{Log, Setup, editor_of, hooks, kind, options_with, redraw, text_of, window_on};
use crate::window_view::NavoWindow;
use crate::window_view::tests::{open_options_sized, press, settle, type_in};
use crate::window_view::viewer::{EDIT_LIMIT, Kinds, PagedKind};

const LOG: &str = "ERROR disk full\nstarted\n";

/// Opens `name` holding `text` with F3, in the editor when F3 opens one.
fn open(cx: &mut TestAppContext, name: &str, text: &str) -> (AnyWindowHandle, Entity<NavoWindow>) {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    fs.add_file(&format!("home/{name}"), 3);
    fs.add_folder("dest");
    let (window, view, _) = window_on(cx, Arc::new(fs), Setup::new(text.as_bytes().to_vec()));
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    (window, view)
}

/// The language the open body reports, or `None` when it has none.
fn language(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> Option<String> {
    view.read_with(cx, |view, cx| {
        let body = view.viewer.as_ref()?.body.as_ref()?;
        body.language(cx).map(|name| name.to_string())
    })
}

/// The texts of the rows of the open viewer menu, empty when none is open.
fn rows(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> Vec<String> {
    view.read_with(cx, |view, cx| {
        let menu = view.viewer_menu_view()?;
        let items = menu.read(cx).menu().items.clone();
        Some(items.into_iter().map(|item| item.text).collect())
    })
    .unwrap_or_default()
}

/// Whether the open menu lists every wanted row.
fn lists(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> bool {
    let rows = rows(cx, view);
    ["Plain text", "Log", "xml", "a2l"]
        .iter()
        .all(|wanted| rows.iter().any(|row| row == wanted))
        && rows
            .first()
            .is_some_and(|auto| auto.starts_with("Auto (detected: "))
}

/// Picks `text` in the language picker opened with Ctrl+Shift+L.
fn choose(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<NavoWindow>, text: &str) {
    press(cx, window, view, "ctrl-shift-l");
    run_item(cx, window, text);
    settle(cx, view);
    redraw(cx, window);
}

#[gpui_kit::test]
fn ctrl_shift_l_opens_the_picker_with_the_rows(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "app.log", LOG);
    press(cx, window, &view, "ctrl-shift-l");
    assert!(lists(cx, &view), "{:?}", rows(cx, &view));
    press_key(cx, window, "escape");
    assert!(rows(cx, &view).is_empty());
}

#[gpui_kit::test]
fn the_view_menu_entry_opens_the_picker(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "app.log", LOG);
    press(cx, window, &view, "alt-v");
    assert!(rows(cx, &view).iter().any(|row| row == "Language\u{2026}"));
    run_item(cx, window, "Language\u{2026}");
    assert!(lists(cx, &view), "{:?}", rows(cx, &view));
}

#[gpui_kit::test]
fn a_click_on_the_header_button_opens_the_picker(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "app.log", LOG);
    redraw(cx, window);
    cx.update_window(window, |_, window, cx| window.click("viewer-language", cx))
        .expect("window");
    settle(cx, &view);
    assert!(lists(cx, &view), "{:?}", rows(cx, &view));
}

#[gpui_kit::test]
fn plain_text_on_a_log_file_switches_the_language_to_text(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "app.log", LOG);
    assert_eq!(language(cx, &view).as_deref(), Some("log"));
    choose(cx, window, &view, "Plain text");
    assert_eq!(language(cx, &view).as_deref(), Some("text"));
}

#[gpui_kit::test]
fn log_on_an_arxml_file_switches_the_language_and_auto_still_names_xml(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "ecu.arxml", "<AUTOSAR></AUTOSAR>\n");
    press(cx, window, &view, "alt-1");
    redraw(cx, window);
    assert_eq!(language(cx, &view).as_deref(), Some("xml"));
    choose(cx, window, &view, "Log");
    assert_eq!(language(cx, &view).as_deref(), Some("log"));
    press(cx, window, &view, "ctrl-shift-l");
    assert_eq!(
        rows(cx, &view).first().map(String::as_str),
        Some("Auto (detected: xml)")
    );
}

#[gpui_kit::test]
fn unsaved_edits_trigger_the_guard_until_the_choice_is_repeated(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "app.log", LOG);
    press(cx, window, &view, "ctrl-e");
    type_in(cx, window, &view, "x");
    choose(cx, window, &view, "Plain text");
    assert_eq!(language(cx, &view).as_deref(), Some("log"), "guarded");
    let editor = editor_of(cx, &view).expect("the editor stays open");
    assert!(editor.read_with(cx, |state, _| state.value().starts_with('x')));
    choose(cx, window, &view, "Plain text");
    assert_eq!(language(cx, &view).as_deref(), Some("text"));
}

/// Opens a log past the edit limit, so it shows as paged text.
fn open_huge(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<NavoWindow>) {
    open_huge_sized(cx, px(1000.))
}

/// Opens the huge log in a window `width` wide.
pub(super) fn open_huge_sized(
    cx: &mut TestAppContext,
    width: Pixels,
) -> (AnyWindowHandle, Entity<NavoWindow>) {
    let files = hooks(Setup::huge(LOG.as_bytes().to_vec()), &Log::default());
    let options = options_with(
        fake_with("big.log", EDIT_LIMIT + 1),
        files,
        Kinds::builtin(),
    );
    let (window, view) = open_options_sized(cx, options, width);
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    (window, view)
}

#[gpui_kit::test]
fn on_a_huge_file_the_choice_survives_hex_and_back_to_text(cx: &mut TestAppContext) {
    let (window, view) = open_huge(cx);
    assert_eq!(language(cx, &view).as_deref(), Some("log"));
    choose(cx, window, &view, "Plain text");
    let chosen = language(cx, &view);
    assert_eq!(chosen.as_deref(), Some("plain"));
    press(cx, window, &view, "alt-3");
    redraw(cx, window);
    assert_eq!(kind(cx, &view), Some(PagedKind::Hex));
    press(cx, window, &view, "alt-1");
    redraw(cx, window);
    assert_eq!(kind(cx, &view), Some(PagedKind::Text));
    assert_eq!(language(cx, &view), chosen);
}

#[gpui_kit::test]
fn in_hex_the_shortcut_shows_the_text_view_note_and_opens_no_picker(cx: &mut TestAppContext) {
    let (window, view) = open_huge(cx);
    press(cx, window, &view, "alt-3");
    redraw(cx, window);
    assert_eq!(kind(cx, &view), Some(PagedKind::Hex));
    assert_eq!(language(cx, &view), None, "Hex has no language");
    press(cx, window, &view, "ctrl-shift-l");
    assert!(rows(cx, &view).is_empty(), "{:?}", rows(cx, &view));
    assert_eq!(kind(cx, &view), Some(PagedKind::Hex), "still Hex");
    let note = text_of(cx, window, "paged-status");
    assert!(note.contains("Alt+1 switches"), "{note}");
}

#[gpui_kit::test]
fn a_click_on_the_paged_header_button_opens_the_picker(cx: &mut TestAppContext) {
    let (window, view) = open_huge(cx);
    cx.update_window(window, |_, window, cx| window.click("viewer-language", cx))
        .expect("window");
    settle(cx, &view);
    assert!(lists(cx, &view), "{:?}", rows(cx, &view));
}

#[gpui_kit::test]
fn escape_from_the_picker_returns_focus_to_the_find_field(cx: &mut TestAppContext) {
    let (window, view) = open_huge(cx);
    press(cx, window, &view, "ctrl-f");
    type_in(cx, window, &view, "ab");
    press(cx, window, &view, "ctrl-shift-l");
    assert!(lists(cx, &view), "{:?}", rows(cx, &view));
    press_key(cx, window, "escape");
    assert!(rows(cx, &view).is_empty());
    type_in(cx, window, &view, "c");
    let typed = with_paged(cx, &view, |paged, cx| paged.bar_text(cx));
    assert_eq!(
        typed.as_deref(),
        Some("abc"),
        "the keys reach the open Find"
    );
}

#[gpui_kit::test]
fn escape_from_the_picker_opened_by_the_view_menu_returns_focus_to_the_find_field(
    cx: &mut TestAppContext,
) {
    let (window, view) = open_huge(cx);
    press(cx, window, &view, "ctrl-f");
    type_in(cx, window, &view, "ab");
    press(cx, window, &view, "alt-v");
    run_item(cx, window, "Language\u{2026}");
    assert!(lists(cx, &view), "{:?}", rows(cx, &view));
    press_key(cx, window, "escape");
    assert!(rows(cx, &view).is_empty());
    type_in(cx, window, &view, "c");
    let typed = with_paged(cx, &view, |paged, cx| paged.bar_text(cx));
    assert_eq!(
        typed.as_deref(),
        Some("abc"),
        "the keys reach the open Find"
    );
}

#[gpui_kit::test]
fn a_language_chosen_while_following_keeps_following(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "app.log", LOG);
    press(cx, window, &view, "alt-f");
    redraw(cx, window);
    assert_eq!(text_of(cx, window, "paged-follow"), "Following");
    choose(cx, window, &view, "Plain text");
    assert_eq!(kind(cx, &view), Some(PagedKind::Text), "not the editor");
    assert_eq!(text_of(cx, window, "paged-follow"), "Following");
    assert_eq!(language(cx, &view).as_deref(), Some("plain"));
}

#[gpui_kit::test]
fn a_discard_confirmation_does_not_outlive_its_editor(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "app.log", LOG);
    press(cx, window, &view, "ctrl-e");
    type_in(cx, window, &view, "x");
    choose(cx, window, &view, "Plain text");
    choose(cx, window, &view, "Plain text");
    assert_eq!(language(cx, &view).as_deref(), Some("text"), "confirmed");
    press(cx, window, &view, "ctrl-e");
    type_in(cx, window, &view, "y");
    choose(cx, window, &view, "Log");
    assert_eq!(language(cx, &view).as_deref(), Some("text"), "asks again");
    let editor = editor_of(cx, &view).expect("the editor stays open");
    assert!(editor.read_with(cx, |state, _| state.value().starts_with('y')));
}

#[gpui_kit::test]
fn a_confirmation_is_for_the_choice_that_was_asked(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "app.log", LOG);
    press(cx, window, &view, "ctrl-e");
    type_in(cx, window, &view, "x");
    choose(cx, window, &view, "Plain text");
    choose(cx, window, &view, "xml");
    assert_eq!(
        language(cx, &view).as_deref(),
        Some("log"),
        "another choice asks"
    );
    let editor = editor_of(cx, &view).expect("the editor stays open");
    assert!(editor.read_with(cx, |state, _| state.value().starts_with('x')));
}
