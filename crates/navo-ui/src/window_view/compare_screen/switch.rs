//! Alt+V Text / Bytes on the file diff (V41b): a text pair reopens in the hex body at the byte
//! offset of its top line, and back in text at the line that holds the hex body's top offset.

use gpui_kit::{Action, Stateful};
use navo_l10n::tr;

use super::copy::FileBars;
use super::file::FileView;
use super::hex::visible_rows;
use super::{FileScreen, FileState};
use crate::commands::compare::{ViewBytes, ViewText};
use crate::compare_diff::PairKind;
use crate::prelude::*;
use crate::tokens::{self, Theme};
use crate::window_view::NavoWindow;
use crate::window_view::viewer::MenuEntry;

/// Which view the pair was forced into and the offset the switch in flight must land on.
#[derive(Default)]
pub(super) struct Switch {
    /// Whether the pair opens as bytes whatever its kind.
    bytes: bool,
    /// The left byte offset to show first; set from the request until its `Opened` lands.
    waiting: Option<u64>,
    /// The text view kept drawn until the hex body has its first rows.
    held: Option<FileView>,
}

impl Switch {
    /// Takes the offset a landing `Opened` must show first.
    pub(super) fn landed(&mut self) -> Option<u64> {
        self.waiting.take()
    }

    /// Keeps the text view that a landed switch to bytes replaces, so no frame is blank.
    pub(super) fn hold(&mut self, view: Option<FileView>) {
        self.held = view;
    }

    /// Drops the text view kept for the switch.
    pub(super) fn release(&mut self) {
        self.held = None;
    }

    /// Forgets the forced bytes view; true when it was set.
    pub(super) fn unforce(&mut self) -> bool {
        std::mem::take(&mut self.bytes)
    }
}

/// The hex body of a binary pair; after Alt+V Show bytes the text view stays drawn instead
/// until the hex body has its first rows (it asks for them meanwhile), so no frame is blank.
pub(super) fn bytes_body(
    file: &mut FileScreen,
    (t, window, bars): (&Theme, &Window, FileBars),
    cx: &mut Context<NavoWindow>,
) -> Option<AnyElement> {
    let hex = file.hex.as_ref()?;
    if hex.ready(visible_rows(t, window)) {
        file.switch.release();
        return Some(hex.body(t, window, cx));
    }
    let Some(old) = file.switch.held.as_mut() else {
        return Some(hex.body(t, window, cx));
    };
    hex.request_later(window, cx);
    Some(old.list(file.look.layout_in(window), bars, cx))
}

/// Whether the file diff holds an opened text pair, the only kind Alt+V can show as bytes.
fn text_pair(file: &FileScreen) -> bool {
    let kind = match &file.state {
        FileState::Opened { kind, .. } => Some(*kind),
        _ => None,
    };
    kind == Some(PairKind::Text)
}

/// The View menu entries that switch the open pair: "Show bytes" on a text pair, "Show text" on
/// a text pair shown as bytes, none otherwise (a binary pair has no text to go back to).
pub(super) fn entries(file: &FileScreen) -> Vec<MenuEntry> {
    let run = |id: &str, action: Box<dyn Action>| MenuEntry::Run(tr(id).into(), action, false);
    match (
        text_pair(file) && !file.switch.bytes,
        file.hex.is_some() && file.switch.bytes,
    ) {
        (true, _) => vec![
            MenuEntry::Break,
            run("cmd-compare-view-bytes", Box::new(ViewBytes)),
        ],
        (_, true) => vec![
            MenuEntry::Break,
            run("cmd-compare-view-text", Box::new(ViewText)),
        ],
        _ => Vec::new(),
    }
}

impl FileScreen {
    /// Clears a forced bytes view when a reload lands as anything but a binary pair (the file
    /// grew past the whole-file limit, say), so the menu and later reloads follow the file.
    pub(super) fn drop_unfit_bytes(&mut self, kind: PairKind) {
        if matches!(kind, PairKind::Binary) || !self.switch.unforce() {
            return;
        }
        if let Some(id) = self.active.checked_add(1) {
            (self.expected, self.active) = (Some(id), id);
            self.control.view_as(id, false);
        }
    }
}

impl NavoWindow {
    /// Binds the two switch commands on the file diff's `root`.
    pub(super) fn switch_actions(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &ViewBytes, window, cx| {
            view.switch_view(true, window, cx);
        }))
        .on_action(cx.listener(|view, _: &ViewText, window, cx| {
            view.switch_view(false, window, cx);
        }))
    }

    /// Reopens the pair as bytes (`bytes`) or as text at the offset on top now. A text pair can
    /// go to bytes and a forced one back; anything else, or a switch already in flight, is
    /// ignored. The old body stays drawn until the new one opens.
    fn switch_view(&mut self, bytes: bool, window: &mut Window, cx: &mut Context<Self>) {
        let t = tokens::current(cx);
        let (row_height, visible) = (t.spacing.row_height, visible_rows(t, window));
        let Some(file) = self.compare.screen.as_mut().and_then(|s| s.file.as_mut()) else {
            return;
        };
        if file.switch.waiting.is_some() {
            return;
        }
        let offset = match (bytes, file.view.as_ref(), file.hex.as_ref()) {
            (true, Some(view), _) if text_pair(file) && !file.switch.bytes => {
                let layout = file.look.layout_in(window);
                view.top_offset(layout, row_height)
            }
            (false, _, Some(hex)) if file.switch.bytes => hex.top_offset(visible),
            _ => return,
        };
        let Some(id) = file.next_id() else {
            return cx.notify();
        };
        file.switch.bytes = bytes;
        file.switch.waiting = Some(offset);
        file.control.view_as(id, bytes);
        cx.notify();
    }
}
