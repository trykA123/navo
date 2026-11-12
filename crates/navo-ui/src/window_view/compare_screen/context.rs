//! The right-click menu of the file diff and the Stream (V33, D27): every key action that
//! applies at the cursor line, with its key from the keymap. A right-click moves the cursor to
//! the line first and opens at the pointer; Shift+F10 and the Menu key open at the cursor line.
//! Actions that are not built yet are left out, not greyed.

use gpui_kit::{Stateful, WeakEntity};
use navo_l10n::tr;

use super::FileScreen;
use super::stream::{StreamScreen, item_of};
use crate::commands::compare::ShowMenu;
use crate::compare_diff::{
    CardRow, CopyDirection, LongHit, PairKind, PickSide, ShownRow, Stop, StreamRow,
};
use crate::prelude::*;
use crate::tokens;
use crate::window_view::ScreenOwner;
use crate::window_view::viewer::MenuEntry;

mod entries;

use entries::{Copying, FileFacts, StreamFacts, file_entries, stream_entries};

/// The change and the lines a file diff row shows, for a right-click on it.
#[derive(Debug, Clone, Copy)]
pub(super) struct RowHit {
    hunk: Option<u32>,
    left: Option<u32>,
    right: Option<u32>,
}

impl RowHit {
    /// What `row` shows.
    pub(super) fn of(row: &ShownRow) -> Self {
        Self {
            hunk: row.hunk,
            left: row.left.map(|(line, _)| line),
            right: row.right.map(|(line, _)| line),
        }
    }
}

/// Wraps the drawn file diff row `shown` so a right-click on it moves the cursor there and
/// opens the menu at the pointer.
pub(super) fn on_right_click(
    weak: &WeakEntity<ScreenOwner>,
    shown: AnyElement,
    hit: RowHit,
) -> AnyElement {
    let weak = weak.clone();
    div()
        .w_full()
        .on_mouse_down(MouseButton::Right, move |event, window, cx| {
            cx.stop_propagation();
            let at = event.position;
            let _ = weak.update(cx, |view, cx| view.menu_at_file_row((hit, at), window, cx));
        })
        .child(shown)
        .into_any_element()
}

impl FileScreen {
    /// What the menu may offer at the cursor line of the open pair.
    fn menu_facts(&self) -> FileFacts {
        if let Some(hex) = &self.hex {
            return FileFacts {
                changes: hex.region_count() > 0,
                ..FileFacts::default()
            };
        }
        let Some(view) = &self.view else {
            let Some(large) = &self.large else {
                return FileFacts::default();
            };
            return FileFacts {
                changes: large.has_changes(),
                bytes_here: self.switch.aimed(),
                ..FileFacts::default()
            };
        };
        let text = view.text();
        let copyable = view
            .current()
            .and_then(|at| text.hunks.get(at))
            .is_some_and(|hunk| hunk.digest.is_some());
        let copy = match self.pick.picked_side() {
            Some(PickSide::Left) => Copying::Picked(CopyDirection::ToRight),
            Some(PickSide::Right) => Copying::Picked(CopyDirection::ToLeft),
            None if copyable => Copying::Change,
            None => Copying::None,
        };
        FileFacts {
            copy,
            remove: self.pick.has_lines(),
            pick: self.pick.line_state(text, view.current()),
            changes: view.change_count() > 0,
            bytes_here: self.switch.aimed(),
        }
    }
}

#[cfg(test)]
impl FileScreen {
    /// The labels of the entries the menu would list for the cursor line now.
    pub(super) fn menu_labels(&self) -> Vec<String> {
        file_entries(&self.menu_facts())
            .into_iter()
            .filter_map(|entry| match entry {
                MenuEntry::Run(label, ..) => Some(label.to_string()),
                _ => None,
            })
            .collect()
    }
}

impl StreamScreen {
    /// What the menu may offer at the current stop.
    fn menu_facts(&self) -> StreamFacts {
        let stop = self.model.cursor();
        let item = self.model.items().get(stop.item);
        let copy = item.is_some_and(|item| {
            let found = self.model.card(item.id).zip(stop.hunk);
            found.is_some_and(|(card, hunk)| {
                let copyable = card
                    .hunks
                    .get(hunk as usize)
                    .is_some_and(|h| h.digest.is_some());
                card.kind == PairKind::Text && copyable && self.requests.contains_key(&item.id)
            })
        });
        StreamFacts {
            item: item.is_some(),
            copy,
        }
    }

    /// Puts the current stop on the row `at` (its change, else its card) for a right-click.
    fn point_at_row(&mut self, at: usize) {
        let Some(row) = self.rows.get(at).copied() else {
            return;
        };
        let Some(item) = item_of(&row) else {
            return;
        };
        let card = self
            .model
            .items()
            .get(item)
            .and_then(|i| self.model.card(i.id));
        let hunk = match (row, card) {
            (StreamRow::Line { card_row, .. }, Some(card)) => {
                match card.rows.get(card_row as usize) {
                    Some(CardRow::Line { hunk, .. }) => Some(*hunk),
                    _ => None,
                }
            }
            _ => None,
        };
        if !self.model.set_stop(Stop { item, hunk }) {
            self.model.jump_to(item);
        }
        self.sync_rows();
    }
}

impl ScreenOwner {
    /// Binds Shift+F10 and the Menu key (open the menu at the cursor line) on a screen's `root`.
    pub(super) fn menu_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &ShowMenu, window, cx| {
            view.open_compare_menu(None, window, cx);
        }))
    }

    /// A right-click on a file diff row: the cursor goes to its change and line, then the menu
    /// opens at `at`. A Large row has no cursor to move, but it still notes its aim (V41e).
    fn menu_at_file_row(
        &mut self,
        (hit, at): (RowHit, Point<Pixels>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(file) = self.compare.screen.as_mut().and_then(|s| s.file.as_mut()) {
            point_text_row(file, hit);
            file.aim_at(LongHit {
                left: hit.left,
                right: hit.right,
            });
        }
        self.compare.diff.copy.note = None;
        self.open_compare_menu(Some(at), window, cx);
    }

    /// A right-click on Stream row `row`: the cursor goes there, then the menu opens at `at`.
    pub(super) fn menu_at_stream_row(
        &mut self,
        (row, at): (usize, Point<Pixels>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(stream) = self.stream_mut() {
            stream.point_at_row(row);
        }
        self.compare.diff.copy.note = None;
        self.open_compare_menu(Some(at), window, cx);
    }

    /// Opens the menu of the top screen at `at`, or under the cursor line when `at` is `None`.
    fn open_compare_menu(
        &mut self,
        at: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let anchor = at.unwrap_or_else(|| self.cursor_anchor(window, cx));
        if at.is_none()
            && let Some(file) = self.compare.screen.as_mut().and_then(|s| s.file.as_mut())
        {
            file.aim_from_cursor();
        }
        let Some(screen) = self.compare.screen.as_ref() else {
            return;
        };
        let items: Vec<MenuEntry> = match (&screen.file, &screen.stream) {
            (Some(file), _) => file_entries(&file.menu_facts()),
            (None, Some(stream)) => stream_entries(&stream.menu_facts()),
            (None, None) => return,
        };
        self.show_menu((items, "compare-menu-title", anchor), window, cx);
    }

    /// The window point just under the cursor line, kept inside the window.
    fn cursor_anchor(&mut self, window: &Window, cx: &App) -> Point<Pixels> {
        let t = tokens::current(cx);
        let (head, row) = (t.spacing.path_height, t.spacing.row_height);
        let viewport = window.viewport_size();
        let from_file = self.compare.screen.as_mut().and_then(|s| s.file.as_mut());
        let y = match from_file {
            Some(file) => {
                let layout = file.look.layout_in(window);
                let text = file
                    .view
                    .as_ref()
                    .map(|view| std::sync::Arc::clone(view.text()));
                let spot = text.and_then(|text| file.pick.cursor_spot(&text));
                let bottom = file.view.as_mut().map(|v| v.line_bottom(layout, spot, row));
                head + bottom.unwrap_or_default()
            }
            None => self.stream_cursor_bottom().unwrap_or(viewport.height / 2.),
        };
        let y = y.max(head * 2.).min(viewport.height - head * 2.);
        point(t.spacing.rail_width, y)
    }

    /// The bottom edge of the Stream's current stop, in window px, once its row is drawn.
    fn stream_cursor_bottom(&mut self) -> Option<Pixels> {
        let stream = self.stream_mut()?;
        let row = stream.cursor_row()?;
        Some(stream.list.bounds_for_item(row)?.bottom())
    }
}

/// One entry: the label of Fluent id `id`, running `action`.
pub(super) fn run(id: &str, action: impl gpui_kit::Action) -> MenuEntry {
    MenuEntry::Run(tr(id).into(), Box::new(action), false)
}

/// Moves the text pair's cursor onto the clicked row: its change, then its line. A pair with no
/// text view (Large, image) has no cursor, so this does nothing for it.
fn point_text_row(file: &mut FileScreen, hit: RowHit) {
    let Some(view) = file.view.as_mut() else {
        return;
    };
    if let Some(hunk) = hit.hunk {
        view.set_current(hunk as usize);
    }
    let side = file.pick.side_for(hit.left.is_some());
    let line = match side {
        PickSide::Left => hit.left,
        PickSide::Right => hit.right,
    };
    if let Some(line) = line {
        file.pick.point(view.text(), (side, line));
    }
}
