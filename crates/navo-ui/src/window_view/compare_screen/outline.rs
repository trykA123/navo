//! The Stream's outline drawer (V21): Ctrl+B shows the files grouped by folder with status
//! mark, Viewed tick and search hits. This file holds its state, lines and keys.

mod view;

use gpui_kit::{FocusHandle, ListOffset, ScrollHandle, Stateful};

use super::stream::{StreamScreen, item_of};
use crate::commands::compare::Changes;
use crate::compare_diff::{Layout, StreamModel, StreamRow};
use crate::prelude::*;
use crate::window_view::NavoWindow;
use crate::window_view::viewer::{ListDown, ListGo, ListUp};

pub(super) use view::drawer;

/// The drawer's own state: focus (so its keys apply), scroll, and the row Up/Down picked.
pub(super) struct Outline {
    pub(super) focus: FocusHandle,
    scroll: ScrollHandle,
    /// The item index Up/Down moved to; `None` until they do, then Enter uses the item in view.
    selected: Option<usize>,
}

impl Outline {
    /// A closed outline with its own focus handle.
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            selected: None,
        }
    }
}

/// One line of the drawer.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Line {
    /// A folder heading; empty for the top level.
    Folder(String),
    /// The file of item `n`.
    File(usize),
    /// The count of same-after-rules files at the end.
    Rules(usize),
}

/// The drawer's lines for the items shown now: a heading whenever the folder changes, then one
/// line per file, then the rules count (left out while a search narrows the Stream).
fn lines(model: &StreamModel) -> Vec<Line> {
    let mut out = Vec::new();
    let mut last: Option<&str> = None;
    for at in model.visible() {
        let dir = model.items()[at]
            .path
            .rsplit_once('/')
            .map_or("", |(d, _)| d);
        if last != Some(dir) {
            out.push(Line::Folder(dir.to_owned()));
            last = Some(dir);
        }
        out.push(Line::File(at));
    }
    if !model.searching() && model.rules() > 0 {
        out.push(Line::Rules(model.rules()));
    }
    out
}

impl StreamScreen {
    /// The item in view: the one the list's top row belongs to, else the cursor's item.
    pub(super) fn item_in_view(&self) -> usize {
        let top = self.list.logical_scroll_top().item_ix;
        self.rows
            .get(top)
            .and_then(item_of)
            .unwrap_or(self.model.cursor().item)
    }

    /// Scrolls the list so the card header of `item` is its top row.
    fn scroll_to_header(&self, item: usize) {
        if let Some(row) = self.rows.iter().position(|r| *r == StreamRow::Header(item)) {
            self.list.scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: px(0.),
            });
        }
    }

    /// Moves the drawer's pick one shown file up (`-1`) or down (`1`) and scrolls it into view.
    fn move_pick(&mut self, step: i32) {
        let visible = self.model.visible();
        let from = self.outline.selected.unwrap_or_else(|| self.item_in_view());
        let here = visible.iter().position(|&at| at == from).unwrap_or(0);
        let to = here
            .saturating_add_signed(step as isize)
            .min(visible.len().saturating_sub(1));
        let Some(&item) = visible.get(to) else {
            return;
        };
        self.outline.selected = Some(item);
        if let Some(line) = lines(&self.model)
            .iter()
            .position(|l| *l == Line::File(item))
        {
            self.outline.scroll.scroll_to_item(line);
        }
    }
}

impl NavoWindow {
    /// Binds the drawer's keys on `root`: Ctrl+B opens or closes it; Up, Down and Enter work
    /// while it holds focus.
    pub(super) fn outline_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &Changes, window, cx| {
            view.outline_toggle(window, cx);
        }))
        .on_action(cx.listener(|view, _: &ListUp, _, cx| view.outline_move(-1, cx)))
        .on_action(cx.listener(|view, _: &ListDown, _, cx| view.outline_move(1, cx)))
        .on_action(cx.listener(|view, _: &ListGo, window, cx| {
            let at = view.stream_mut().map(|stream| {
                stream
                    .outline
                    .selected
                    .unwrap_or_else(|| stream.item_in_view())
            });
            if let Some(at) = at {
                view.outline_jump(at, window, cx);
            }
        }))
    }

    /// Opens the drawer and focuses it, or closes it and gives focus back to the cards.
    fn outline_toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(stream) = self.stream_mut() else {
            return;
        };
        let open = !stream.model.outline_open();
        stream.model.set_outline(open);
        stream.outline.selected = None;
        let focus = if open {
            stream.outline.focus.clone()
        } else {
            stream.focus.clone()
        };
        window.defer(cx, move |window, cx| window.focus(&focus, cx));
        cx.notify();
    }

    /// Up or Down in the drawer.
    fn outline_move(&mut self, step: i32, cx: &mut Context<Self>) {
        if let Some(stream) = self.stream_mut() {
            stream.move_pick(step);
            cx.notify();
        }
    }

    /// Jumps to the card of `item`: it becomes the cursor and the top row. Below 900 px the
    /// drawer closes and focus goes back to the cards.
    pub(super) fn outline_jump(
        &mut self,
        item: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(stream) = self.stream_mut() else {
            return;
        };
        if !stream.model.jump_to(item) {
            return;
        }
        stream.scroll_to_header(item);
        stream.outline.selected = None;
        if stream.layout == Layout::Unified {
            stream.model.set_outline(false);
            let focus = stream.focus.clone();
            window.defer(cx, move |window, cx| window.focus(&focus, cx));
        }
        self.settle_stream_focus(cx);
        cx.notify();
    }
}

#[cfg(test)]
impl NavoWindow {
    /// The item the outline marks as in view, for tests.
    pub(in crate::window_view) fn outline_in_view(&self) -> Option<usize> {
        let stream = self.compare.screen.as_ref()?.stream.as_ref()?;
        Some(stream.item_in_view())
    }
}

#[cfg(test)]
mod tests;
