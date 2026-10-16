use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use super::structured::fake_with;
use super::{Log, Setup, has_editor, hooks, redraw, window_with};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{press, settle, type_in, viewer_open};
use crate::window_view::viewer::editor::EditorBody;
use crate::window_view::viewer::{FileHooks, Kinds};

type Saved = Arc<Mutex<HashMap<String, bool>>>;

fn split_hooks(source: &str, saved: &Saved) -> FileHooks {
    let files = hooks(Setup::new(source.as_bytes().to_vec()), &Log::default());
    let (load, save) = (saved.clone(), saved.clone());
    FileHooks {
        split: Arc::new(move |extension| load.lock().expect("saved").get(extension).copied()),
        save_split: Arc::new(move |extension, is_shown| {
            save.lock()
                .expect("saved")
                .insert(extension.to_owned(), is_shown);
        }),
        ..files
    }
}

fn open_editing(
    cx: &mut TestAppContext,
    name: &str,
    source: &str,
    saved: &Saved,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let files = split_hooks(source, saved);
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
    let (window, view) = open_editing(cx, "notes.md", NOTES, &Saved::default());
    assert!(shows_preview(cx, window));
    let shown = with_editor(cx, &view, EditorBody::preview_text);
    assert_eq!(shown.as_deref(), Some("Notes\nSome bold text."));
    assert!(preview_has_block(cx, window, 0));
}

#[gpui_kit::test]
fn several_keystrokes_give_one_preview_render(cx: &mut TestAppContext) {
    let (window, view) = open_editing(cx, "notes.md", NOTES, &Saved::default());
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
    let (window, view) = open_editing(cx, "long.md", &source, &Saved::default());
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
fn ctrl_shift_v_toggles_the_split_and_remembers_it_per_extension(cx: &mut TestAppContext) {
    let saved = Saved::default();
    let (window, view) = open_editing(cx, "notes.md", NOTES, &saved);
    press(cx, window, &view, "ctrl-shift-v");
    settle(cx, &view);
    assert!(!shows_preview(cx, window));
    assert_eq!(saved.lock().expect("saved").get("md"), Some(&false));
    let (window, view) = open_editing(cx, "other.md", NOTES, &saved);
    assert!(!shows_preview(cx, window), "md stays without the preview");
    let (markdown, _) = open_editing(cx, "guide.markdown", NOTES, &saved);
    assert!(
        shows_preview(cx, markdown),
        "other extensions keep their own choice"
    );
    press(cx, window, &view, "ctrl-shift-v");
    settle(cx, &view);
    assert!(shows_preview(cx, window));
    assert_eq!(saved.lock().expect("saved").get("md"), Some(&true));
}

#[gpui_kit::test]
fn a_non_markdown_file_has_no_split(cx: &mut TestAppContext) {
    let saved = Saved::default();
    let (window, view) = open_editing(cx, "notes.txt", NOTES, &saved);
    assert!(!shows_preview(cx, window));
    press(cx, window, &view, "ctrl-shift-v");
    settle(cx, &view);
    assert!(!shows_preview(cx, window));
    assert!(saved.lock().expect("saved").is_empty());
}

#[gpui_kit::test]
fn f6_moves_focus_to_the_preview_and_escape_returns_to_the_source(cx: &mut TestAppContext) {
    let (window, view) = open_editing(cx, "notes.md", NOTES, &Saved::default());
    let focus = |cx: &mut TestAppContext| {
        cx.update_window(window, |_, window, cx| {
            with_window_editor(window, &view, cx, |editor, window, cx| {
                editor.focused_side(window, cx)
            })
            .flatten()
        })
        .expect("window")
    };
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
    let (window, view) = open_editing(cx, "notes.md", source, &Saved::default());
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
}
