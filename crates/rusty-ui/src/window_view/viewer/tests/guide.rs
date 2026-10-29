use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};

use super::{editor_text, has_editor, redraw, text_of, window_with};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{fake, press, settle, type_in, viewer_open};
use crate::window_view::viewer::markdown::MarkdownBody;
use crate::window_view::viewer::{FileHooks, Kinds};

fn open_guide(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let (window, view) = window_with(cx, fake(), FileHooks::none(), Kinds::builtin());
    press(cx, window, &view, "shift-f1");
    settle(cx, &view);
    redraw(cx, window);
    (window, view)
}

fn body_of<T>(
    cx: &mut TestAppContext,
    view: &Entity<FerriteWindow>,
    read: impl FnOnce(&MarkdownBody, &gpui_kit::App) -> T,
) -> T {
    view.read_with(cx, |view, cx| {
        let body = view.viewer.as_ref()?.body.as_ref()?;
        Some(read(body.as_any().downcast_ref::<MarkdownBody>()?, cx))
    })
    .expect("the guide is shown in the markdown view")
}

#[gpui_kit::test]
fn shift_f1_opens_the_guide_in_the_markdown_view_under_its_own_title(cx: &mut TestAppContext) {
    let (window, view) = open_guide(cx);
    let rendered = body_of(cx, &view, MarkdownBody::rendered);
    assert!(rendered.starts_with("Navo user guide"), "{rendered}");
    assert!(rendered.contains("Reporting a problem"));
    assert!(
        !rendered.contains("]("),
        "links are rendered, not shown raw"
    );
    assert_eq!(text_of(cx, window, "viewer-title"), "Navo user guide");
}

#[gpui_kit::test]
fn the_guide_is_read_only_and_escape_closes_it(cx: &mut TestAppContext) {
    let (window, view) = open_guide(cx);
    press(cx, window, &view, "f4");
    assert!(!has_editor(cx, &view));
    assert!(viewer_open(cx, &view));
    press(cx, window, &view, "escape");
    assert!(!viewer_open(cx, &view));
}

#[gpui_kit::test]
fn the_guide_source_cannot_be_typed_into(cx: &mut TestAppContext) {
    let (window, view) = open_guide(cx);
    press(cx, window, &view, "1");
    let before = editor_text(cx, &view);
    assert!(before.starts_with("# Navo user guide"));
    type_in(cx, window, &view, "x");
    assert_eq!(editor_text(cx, &view), before);
}

#[gpui_kit::test]
fn a_contents_link_jumps_to_its_heading(cx: &mut TestAppContext) {
    let (window, view) = open_guide(cx);
    for _ in 0..4 {
        press(cx, window, &view, "tab");
    }
    press(cx, window, &view, "enter");
    redraw(cx, window);
    assert_eq!(
        body_of(cx, &view, MarkdownBody::top_text),
        "Working with files"
    );
    assert_eq!(text_of(cx, window, "markdown-status"), "");
}
