//! The editor swap for large text files: the paged Text view shows at once, the whole text is
//! read on a background task after that first frame, and the editor replaces the paged view.

use gpui_kit::component::input::EditorState;
use gpui_kit::{AnyView, Focusable, Subscription, WeakEntity, point};

use super::editor::{EditorBody, EditorFile, SplitChoice};
use super::open::{Opening, start};
use super::paged::{PagedBody, PagedKind, PagedView};
use super::syntax::Highlight;
use super::{BodyKeys, FileText, Viewer, ViewerBody};
use crate::prelude::*;
use crate::window_view::NavoWindow;

/// Editable text files above this size (bytes) open paged first and get the editor later.
pub(super) const SWAP_FROM: u64 = 1024 * 1024;

/// What the editor needs besides the text, chosen while the file opened.
pub(super) struct EditorLater {
    pub highlight: Highlight,
    pub split: Option<SplitChoice>,
    /// Editing starts once the editor is shown: asked for when the file opened.
    pub edit: bool,
}

/// A pending swap: the editor to build, the paged view it replaces and the read text.
pub(super) struct Swap {
    later: Option<EditorLater>,
    /// Editing starts once the editor is shown: asked for when the file opened, or Ctrl+E since.
    edit: bool,
    paged: Entity<PagedView>,
    file: Option<FileText>,
    staged: Option<Staged>,
    _reading: Option<Opening>,
    retry: Option<Subscription>,
}

/// The built editor, laid out unseen under the paged view until its top line is set.
struct Staged {
    body: EditorBody,
    /// The paged view's top (byte offset) the editor is pinned to.
    top: u64,
    /// True while the layout, pin and show frames are under way.
    pinning: bool,
    /// Pins that found no layout yet.
    misses: u8,
}

impl Viewer {
    /// Remembers an edit request for the editor still to come; false when no swap is pending.
    pub(super) fn edit_after_swap(&mut self) -> bool {
        let Some(swap) = &mut self.swap else {
            return false;
        };
        swap.edit = true;
        true
    }

    /// The staged editor, drawn hidden under the paged view so it gets a layout.
    pub(super) fn staged_view(&self) -> Option<AnyView> {
        let staged = self.swap.as_ref()?.staged.as_ref()?;
        Some(staged.body.view())
    }
}

/// True while the paged view holds state the editor cannot take over: an open bar or list,
/// Follow, or the Hex layout.
fn is_busy(viewer: &Viewer, paged: &Entity<PagedView>, cx: &App) -> bool {
    let typing = viewer
        .body
        .as_ref()
        .is_some_and(|body| body.is_typing(cx) || matches!(body.keys(cx), BodyKeys::Modal(_)));
    let view = paged.read(cx);
    typing || view.is_following() || view.kind != PagedKind::Text
}

/// Scrolls a laid-out editor so buffer row `row` is its top row; false before the first layout.
fn scroll_to_row(state: &mut EditorState, row: usize, cx: &mut Context<EditorState>) -> bool {
    let Some(height) = state.line_height() else {
        return false;
    };
    let shown = state.display_map().buffer_line_to_display_row(row);
    let x = state.scroll_offset().x;
    state.set_scroll_offset(point(x, -(height * shown as f32)), cx);
    true
}

impl NavoWindow {
    /// Plans the swap for the paged body just shown; the read starts on the next frame, so
    /// the paged view draws first.
    pub(super) fn plan_swap(
        &mut self,
        later: Option<EditorLater>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(later), Some(viewer)) = (later, &mut self.viewer) else {
            return;
        };
        let Some(paged) = viewer.body.as_deref().and_then(PagedBody::of) else {
            return;
        };
        viewer.swap = Some(Swap {
            edit: later.edit,
            later: Some(later),
            paged: paged.0.clone(),
            file: None,
            staged: None,
            _reading: None,
            retry: None,
        });
        let this = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            let _ = this.update(cx, |view, cx| view.read_for_swap(window, cx));
        });
    }

    /// Reads the whole file for a pending swap on a background task.
    fn read_for_swap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(viewer) = self.viewer.as_ref().filter(|viewer| viewer.swap.is_some()) else {
            return;
        };
        let request = self.request(viewer);
        let reading = start(
            window,
            cx,
            move |_| request.hooks.read_with(&request.location, request.encoding),
            Self::swap_read,
        );
        if let Some(swap) = self.viewer.as_mut().and_then(|viewer| viewer.swap.as_mut()) {
            swap._reading = Some(reading);
        }
    }

    /// Keeps the read text and swaps when the paged view is free; a failed read keeps the
    /// paged view and says why.
    fn swap_read(
        &mut self,
        read: Result<FileText, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(viewer) = self.viewer.as_mut() else {
            return;
        };
        let Some(swap) = viewer.swap.as_mut() else {
            return;
        };
        match read {
            Ok(file) => swap.file = Some(file),
            Err(message) => {
                viewer.swap = None;
                viewer.say(message, cx);
                cx.notify();
                return;
            }
        }
        self.try_swap(window, cx);
    }

    /// Swaps the editor in, or waits for the next change of a busy paged view.
    fn try_swap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(viewer) = &mut self.viewer else {
            return;
        };
        let ready = |swap: &&Swap| swap.file.is_some() || swap.staged.is_some();
        let Some(swap) = viewer.swap.as_ref().filter(ready) else {
            return;
        };
        let paged = viewer.body.as_deref().and_then(PagedBody::of);
        if paged.is_none_or(|paged| paged.0 != swap.paged) {
            viewer.swap = None;
            return;
        }
        let paged = swap.paged.clone();
        let view = paged.read(cx);
        let line = view.top_line();
        if line.is_none() && view.index_failed() {
            viewer.swap = None;
            return;
        }
        if let Some(line) = line.filter(|_| !is_busy(viewer, &paged, cx)) {
            self.stage(line, window, cx);
            return;
        }
        if let Some(swap) = viewer.swap.as_mut().filter(|swap| swap.retry.is_none()) {
            swap.retry = Some(cx.observe_in(&paged, window, |view, _, window, cx| {
                view.try_swap(window, cx);
            }));
        }
    }

    /// Builds the editor once, then pins it to the paged view's 1-based top `line` while it is
    /// drawn hidden; the paged view keeps the focus until the editor is shown.
    fn stage(&mut self, line: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(paged) = self.swap_mut().map(|swap| swap.paged.clone()) else {
            return;
        };
        let focused = paged.read(cx).focus_handle(cx).contains_focused(window, cx);
        if self.swap_mut().is_some_and(|swap| swap.staged.is_none()) {
            self.build(window, cx);
        }
        let this = cx.weak_entity();
        let Some(staged) = self.staged_mut().filter(|staged| !staged.pinning) else {
            return;
        };
        staged.top = paged.read(cx).top;
        staged.pinning = true;
        staged.body.go_to_line(line, window, cx);
        let row = usize::try_from(line.saturating_sub(1)).unwrap_or(usize::MAX);
        pin_then_show(this, staged.body.editor(cx), row, window);
        if focused {
            window.focus(&paged.focus_handle(cx), cx);
        }
        cx.notify();
    }

    /// Builds the hidden editor from the read text, with the paged view's marks.
    fn build(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(viewer) = self.viewer.as_mut() else {
            return;
        };
        let Some(swap) = viewer.swap.as_mut() else {
            return;
        };
        let (Some(file), Some(later)) = (swap.file.take(), swap.later.take()) else {
            return;
        };
        let top = swap.paged.read(cx).top;
        viewer.carried = viewer.body.as_ref().and_then(|body| body.carry(cx));
        let Some(viewer) = self.viewer.as_ref() else {
            return;
        };
        let host = self.host(viewer, window, cx);
        let editor = EditorFile {
            file,
            highlight: later.highlight,
            split: later.split,
        };
        let body = EditorBody::new(editor, host, window, cx);
        let Some(viewer) = self.viewer.as_mut() else {
            return;
        };
        viewer.carried = None;
        if let Some(swap) = viewer.swap.as_mut() {
            swap.staged = Some(Staged {
                body,
                top,
                pinning: false,
                misses: 0,
            });
        }
    }

    /// Shows the staged editor once its pin held on an unmoved paged view; otherwise pins it
    /// again, or waits while the paged view is busy. After three missed layouts it shows anyway.
    /// `pin` is the editor that was pinned and whether its pin held; a late frame for an editor
    /// that is no longer staged (another file) is ignored.
    fn show_staged(
        &mut self,
        (editor, pinned): (&Entity<EditorState>, bool),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(swap) = self.swap_mut() else {
            return;
        };
        let top = swap.paged.read(cx).top;
        let Some(staged) = swap.staged.as_mut() else {
            return;
        };
        if staged.body.editor(cx) != *editor {
            return;
        }
        staged.pinning = false;
        staged.misses += u8::from(!pinned);
        let ready = pinned || staged.misses >= 3;
        if !ready || staged.top != top || !self.swap_is_free(cx) {
            self.try_swap(window, cx);
            return;
        }
        if let Some(swap) = self.viewer.as_mut().and_then(|viewer| viewer.swap.take()) {
            self.swap_in(swap, window, cx);
        }
    }

    /// Puts the staged editor where the paged view was, with the paged view's note, any edit
    /// asked for meanwhile, and the focus.
    fn swap_in(&mut self, swap: Swap, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Staged { body, .. }) = swap.staged else {
            return;
        };
        let focused = swap
            .paged
            .read(cx)
            .focus_handle(cx)
            .contains_focused(window, cx);
        if swap.edit {
            body.start_editing(cx);
        }
        if let Some(note) = swap.paged.read(cx).note() {
            body.set_note(note, cx);
        }
        let Some(viewer) = &mut self.viewer else {
            return;
        };
        viewer.body = Some(Box::new(body));
        if focused {
            window.focus(&viewer.focus_target(cx), cx);
        }
        self.show_parts(cx);
        cx.notify();
    }

    /// True when the body is still the swap's paged view and that view is not busy.
    fn swap_is_free(&self, cx: &App) -> bool {
        let Some(viewer) = &self.viewer else {
            return false;
        };
        let Some(swap) = &viewer.swap else {
            return false;
        };
        let paged = viewer.body.as_deref().and_then(PagedBody::of);
        paged.is_some_and(|paged| paged.0 == swap.paged) && !is_busy(viewer, &swap.paged, cx)
    }

    /// The pending swap, if any.
    fn swap_mut(&mut self) -> Option<&mut Swap> {
        self.viewer.as_mut()?.swap.as_mut()
    }

    /// The staged editor of the pending swap, if it is built.
    fn staged_mut(&mut self) -> Option<&mut Staged> {
        self.swap_mut()?.staged.as_mut()
    }
}

/// Waits one frame so the hidden editor gets its first layout, pins its top row, and asks for
/// it to be shown on the frame after, when the pinned scroll has been laid out.
fn pin_then_show(
    this: WeakEntity<NavoWindow>,
    editor: Entity<EditorState>,
    row: usize,
    window: &Window,
) {
    window.on_next_frame(move |window, _| {
        window.on_next_frame(move |window, cx| {
            let pinned = editor.update(cx, |state, cx| scroll_to_row(state, row, cx));
            show_next_frame(this, editor, pinned, window);
        });
    });
}

/// Shows the staged `editor` at the start of the next frame, if it is still the staged one.
fn show_next_frame(
    this: WeakEntity<NavoWindow>,
    editor: Entity<EditorState>,
    pinned: bool,
    window: &Window,
) {
    window.on_next_frame(move |window, cx| {
        let _ = this.update(cx, |view, cx| {
            view.show_staged((&editor, pinned), window, cx)
        });
    });
}
