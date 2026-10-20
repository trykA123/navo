use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, InputEvent as _, Modifiers, MouseButton,
    MouseDownEvent, MouseUpEvent, Pixels, Point, TestAppContext, point, px,
};
use rusty_structured::markdown::MAX_SOURCE;

use super::structured::fake_with;
use super::{Log, Setup, editor_of, has_editor, hooks, press_only, redraw, window_with};
use crate::tokens;
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{press, settle, type_in, viewer_open};
use crate::window_view::viewer::Kinds;
use crate::window_view::viewer::editor::EditorBody;

fn open_editing(
    cx: &mut TestAppContext,
    name: &str,
    source: &str,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let files = hooks(Setup::new(source.as_bytes().to_vec()), &Log::default());
    let fs = fake_with(name, source.len() as u64);
    let (window, view) = window_with(cx, fs, files, Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    settle(cx, &view);
    press(cx, window, &view, "ctrl-e");
    settle(cx, &view);
    redraw(cx, window);
    assert!(has_editor(cx, &view), "{name} opens in the editor");
    (window, view)
}

fn with_editor<T>(
    cx: &mut TestAppContext,
    view: &Entity<FerriteWindow>,
    read: impl FnOnce(&EditorBody, &gpui_kit::App) -> T,
) -> T {
    view.read_with(cx, |view, cx| {
        let body = view.viewer.as_ref()?.body.as_ref()?;
        let editor = body.as_any().downcast_ref::<EditorBody>()?;
        Some(read(editor, cx))
    })
    .expect("the editor is open")
}

fn shows_preview(cx: &mut TestAppContext, window: AnyWindowHandle) -> bool {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find("editor-preview")
            .is_some_and(|found| found.visible())
    })
    .expect("window")
}

fn preview_has_block(cx: &mut TestAppContext, window: AnyWindowHandle, block: usize) -> bool {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within("editor-preview")
            .try_find(("md-block", block))
            .is_some_and(|found| found.visible())
    })
    .expect("window")
}

fn wait(cx: &mut TestAppContext, millis: u64) {
    cx.executor().advance_clock(Duration::from_millis(millis));
    cx.run_until_parked();
}

const NOTES: &str = "# Notes\n\nSome **bold** text.\n";

#[gpui_kit::test]
fn markdown_opens_split_while_editing_with_a_rendered_preview(cx: &mut TestAppContext) {
    let (window, view) = open_editing(cx, "notes.md", NOTES);
    assert!(shows_preview(cx, window));
    let shown = with_editor(cx, &view, EditorBody::preview_text);
    assert_eq!(shown.as_deref(), Some("Notes\nSome bold text."));
    assert!(preview_has_block(cx, window, 0));
}

#[gpui_kit::test]
fn several_keystrokes_give_one_preview_render(cx: &mut TestAppContext) {
    let (window, view) = open_editing(cx, "notes.md", NOTES);
    let renders = |cx: &mut TestAppContext| with_editor(cx, &view, EditorBody::preview_renders);
    let before = renders(cx);
    for key in ["a", "b", "c"] {
        type_in(cx, window, &view, key);
        wait(cx, 50);
    }
    assert_eq!(renders(cx), before, "nothing renders while typing");
    wait(cx, 150);
    assert_eq!(renders(cx), before + 1);
    let shown = with_editor(cx, &view, EditorBody::preview_text).unwrap_or_default();
    assert!(shown.starts_with("abc"), "{shown}");
}

#[gpui_kit::test]
fn the_preview_shows_the_block_of_the_cursor_line(cx: &mut TestAppContext) {
    let source: String = (0..300).map(|at| format!("Paragraph {at}\n\n")).collect();
    let (window, view) = open_editing(cx, "long.md", &source);
    assert!(preview_has_block(cx, window, 0));
    assert!(!preview_has_block(cx, window, 200));
    press(cx, window, &view, "ctrl-g");
    type_in(cx, window, &view, "401");
    press(cx, window, &view, "enter");
    redraw(cx, window);
    assert!(
        preview_has_block(cx, window, 200),
        "line 401 is paragraph 200"
    );
}

#[gpui_kit::test]
fn ctrl_shift_v_toggles_the_split_for_the_open_file_only(cx: &mut TestAppContext) {
    let (window, view) = open_editing(cx, "notes.md", NOTES);
    press(cx, window, &view, "ctrl-shift-v");
    settle(cx, &view);
    assert!(!shows_preview(cx, window));
    let (other, _) = open_editing(cx, "other.md", NOTES);
    assert!(shows_preview(cx, other), "a new file follows the settings");
    press(cx, window, &view, "ctrl-shift-v");
    settle(cx, &view);
    assert!(shows_preview(cx, window));
}

#[gpui_kit::test]
fn a_non_markdown_file_has_no_split(cx: &mut TestAppContext) {
    let (window, view) = open_editing(cx, "notes.txt", NOTES);
    assert!(!shows_preview(cx, window));
    press(cx, window, &view, "ctrl-shift-v");
    settle(cx, &view);
    assert!(!shows_preview(cx, window));
}

#[gpui_kit::test]
fn f6_moves_focus_to_the_preview_and_escape_returns_to_the_source(cx: &mut TestAppContext) {
    let (window, view) = open_editing(cx, "notes.md", NOTES);
    let focus = |cx: &mut TestAppContext| focused_side(cx, window, &view);
    assert_eq!(focus(cx), Some("source"));
    press(cx, window, &view, "f6");
    assert_eq!(focus(cx), Some("preview"));
    press(cx, window, &view, "escape");
    assert_eq!(focus(cx), Some("source"));
    assert!(
        viewer_open(cx, &view),
        "escape in the preview keeps the editor open"
    );
    press(cx, window, &view, "f6");
    press(cx, window, &view, "f6");
    assert_eq!(focus(cx), Some("source"));
}

fn focused_side(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<FerriteWindow>,
) -> Option<&'static str> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        with_window_editor(window, view, cx, |editor, window, cx| {
            editor.focused_side(window, cx)
        })
        .flatten()
    })
    .expect("window")
}

fn with_window_editor<T>(
    window: &mut gpui_kit::Window,
    view: &Entity<FerriteWindow>,
    cx: &mut gpui_kit::App,
    read: impl FnOnce(&EditorBody, &gpui_kit::Window, &gpui_kit::App) -> T,
) -> Option<T> {
    let view = view.read(cx);
    let body = view.viewer.as_ref()?.body.as_ref()?;
    let editor = body.as_any().downcast_ref::<EditorBody>()?;
    Some(read(editor, window, cx))
}

#[gpui_kit::test]
fn a_file_link_in_the_preview_waits_for_unsaved_edits(cx: &mut TestAppContext) {
    let source = "[next](next.md)\n";
    let (window, view) = open_editing(cx, "notes.md", source);
    type_in(cx, window, &view, "x");
    wait(cx, 200);
    press(cx, window, &view, "f6");
    press(cx, window, &view, "tab");
    press(cx, window, &view, "enter");
    settle(cx, &view);
    assert!(has_editor(cx, &view), "the edited file stays open");
    assert_eq!(
        super::text_of(cx, window, "viewer-message"),
        "Save or discard your edits before opening another file."
    );
    press(cx, window, &view, "tab");
    assert_eq!(
        super::text_of(cx, window, "viewer-message"),
        "Ctrl+click or Enter opens next.md",
        "picking a link again replaces the old message"
    );
}

fn large_source(len: usize) -> String {
    let line = format!("{}\n\n", "x".repeat(998));
    let mut source = line.repeat(len / line.len());
    source.push_str(&"x".repeat(len - source.len()));
    source
}

const NO_PREVIEW: &str = "No preview: Markdown over 1 MB.";

#[gpui_kit::test]
fn f6_into_a_refused_preview_and_escape_come_back_to_the_source(cx: &mut TestAppContext) {
    let source = large_source(MAX_SOURCE - 10);
    let (window, view) = open_editing(cx, "big.md", &source);
    type_in(cx, window, &view, &"y".repeat(20));
    wait(cx, 200);
    redraw(cx, window);
    assert_eq!(super::text_of(cx, window, "viewer-message"), NO_PREVIEW);
    assert!(shows_preview(cx, window));
    press(cx, window, &view, "f6");
    assert_eq!(focused_side(cx, window, &view), Some("preview"));
    press(cx, window, &view, "escape");
    assert_eq!(focused_side(cx, window, &view), Some("source"));
    assert!(viewer_open(cx, &view));
}

#[gpui_kit::test]
fn a_preview_refused_while_focused_keeps_the_focus(cx: &mut TestAppContext) {
    let source = large_source(MAX_SOURCE - 10);
    let (window, view) = open_editing(cx, "big.md", &source);
    press(cx, window, &view, "f6");
    let editor = editor_of(cx, &view).expect("the editor");
    let grown = format!("{source}{}", "y".repeat(20));
    cx.update_window(window, |_, window, cx| {
        editor.update(cx, |state, cx| state.replace_all(grown, window, cx));
    })
    .expect("window");
    wait(cx, 200);
    redraw(cx, window);
    assert_eq!(
        super::text_of(cx, window, "editor-preview-note"),
        NO_PREVIEW
    );
    assert_eq!(focused_side(cx, window, &view), Some("preview"));
    press(cx, window, &view, "escape");
    assert_eq!(focused_side(cx, window, &view), Some("source"));
    assert!(viewer_open(cx, &view));
}

const LINKED: &str = "[next](next.md)\n";

fn edit_and_save(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<FerriteWindow>) {
    type_in(cx, window, view, "x");
    press(cx, window, view, "ctrl-s");
    settle(cx, view);
    wait(cx, 200);
    assert_eq!(super::text_of(cx, window, "viewer-message"), "Saved.");
}

#[gpui_kit::test]
fn the_preview_link_status_shows_over_the_save_note(cx: &mut TestAppContext) {
    let (window, view) = open_editing(cx, "notes.md", LINKED);
    edit_and_save(cx, window, &view);
    press(cx, window, &view, "f6");
    press(cx, window, &view, "tab");
    assert_eq!(
        super::text_of(cx, window, "viewer-message"),
        "Ctrl+click or Enter opens next.md"
    );
    press(cx, window, &view, "escape");
    assert_eq!(super::text_of(cx, window, "viewer-message"), "Saved.");
}

#[gpui_kit::test]
fn typing_clears_the_save_note(cx: &mut TestAppContext) {
    let (window, view) = open_editing(cx, "notes.md", LINKED);
    edit_and_save(cx, window, &view);
    type_in(cx, window, &view, "y");
    wait(cx, 200);
    assert_eq!(super::text_of(cx, window, "viewer-message"), "");
}

#[gpui_kit::test]
fn after_saving_enter_on_a_preview_link_opens_the_file(cx: &mut TestAppContext) {
    let (window, view) = open_editing(cx, "notes.md", LINKED);
    edit_and_save(cx, window, &view);
    press(cx, window, &view, "f6");
    press(cx, window, &view, "tab");
    press(cx, window, &view, "enter");
    settle(cx, &view);
    redraw(cx, window);
    assert_eq!(super::text_of(cx, window, "viewer-title"), "next.md");
}

fn ctrl_click(cx: &mut TestAppContext, window: AnyWindowHandle, at: Point<Pixels>) {
    let modifiers = Modifiers {
        control: true,
        ..Modifiers::default()
    };
    let down = MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers,
        click_count: 1,
        first_mouse: false,
    };
    let up = MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers,
        click_count: 1,
    };
    for event in [down.to_platform_input(), up.to_platform_input()] {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.dispatch_event(event, cx);
        })
        .expect("window");
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn after_saving_ctrl_click_on_a_preview_link_opens_the_file(cx: &mut TestAppContext) {
    let (window, view) = open_editing(cx, "notes.md", LINKED);
    edit_and_save(cx, window, &view);
    let link = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let t = tokens::current(cx);
            let block = window.within("editor-preview").find(("md-block", 0usize));
            let first_line = t.spacing.xl + t.typography.line_height * 0.5;
            block.bounds().origin + point(t.spacing.xxl + px(20.), first_line)
        })
        .expect("window");
    ctrl_click(cx, window, link);
    settle(cx, &view);
    redraw(cx, window);
    assert_eq!(super::text_of(cx, window, "viewer-title"), "next.md");
}

#[gpui_kit::test]
fn markdown_over_200_kb_is_parsed_off_the_ui_thread(cx: &mut TestAppContext) {
    let source = large_source(300 * 1024);
    let (window, view) = open_editing(cx, "big.md", &source);
    let renders = |cx: &mut TestAppContext| with_editor(cx, &view, EditorBody::preview_renders);
    assert_eq!(renders(cx), 1);
    press(cx, window, &view, "ctrl-shift-v");
    type_in(cx, window, &view, "z");
    press_only(cx, window, "ctrl-shift-v");
    assert_eq!(renders(cx), 1, "nothing is parsed on the UI thread");
    wait(cx, 1);
    assert_eq!(renders(cx), 2);
    let shown = with_editor(cx, &view, EditorBody::preview_text).unwrap_or_default();
    assert!(shown.starts_with("zxxx"), "{}", &shown[..10]);
}

#[gpui_kit::test]
fn markdown_over_1_mib_opens_with_the_preview_refused(cx: &mut TestAppContext) {
    let source = large_source(MAX_SOURCE + 10);
    let (window, view) = open_editing(cx, "big.md", &source);
    assert!(shows_preview(cx, window));
    assert_eq!(
        super::text_of(cx, window, "editor-preview-note"),
        NO_PREVIEW
    );
    assert_eq!(with_editor(cx, &view, EditorBody::preview_renders), 1);
    let shown = with_editor(cx, &view, EditorBody::preview_text);
    assert_eq!(shown.as_deref(), Some(""));
}

fn wide_table(word: &str) -> String {
    let head = format!("|{}\n", format!(" {word} |").repeat(12));
    let rule = format!("|{}\n", " --- |".repeat(12));
    let row = format!("|{}\n", " x |".repeat(12));
    format!("{head}{rule}{row}\n")
}

fn preview_x(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: (&'static str, usize),
) -> Pixels {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("editor-preview").find(id).bounds().origin.x
    })
    .expect("window")
}

#[gpui_kit::test]
fn a_new_table_does_not_take_the_sideways_scroll_of_another(cx: &mut TestAppContext) {
    let first = wide_table(&"a".repeat(30));
    let (window, view) = open_editing(cx, "tables.md", &first);
    press(cx, window, &view, "f6");
    press(cx, window, &view, "right");
    redraw(cx, window);
    let scrolled = preview_x(cx, window, ("md-row", 0));
    assert!(scrolled < preview_x(cx, window, ("md-table", 0)));
    press(cx, window, &view, "escape");
    type_in(cx, window, &view, &wide_table(&"b".repeat(30)));
    wait(cx, 200);
    redraw(cx, window);
    let shown = with_editor(cx, &view, EditorBody::preview_text).unwrap_or_default();
    assert!(shown.starts_with("bbb"), "{shown}");
    assert_eq!(
        preview_x(cx, window, ("md-row", 0)),
        preview_x(cx, window, ("md-table", 0)),
        "the new first table starts at its first column"
    );
}
