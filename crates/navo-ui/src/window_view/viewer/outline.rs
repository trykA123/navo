//! The F3 editor's outline popup (Ctrl+Shift+O, View > Outline…): the file's symbols in the
//! filterable menu the language picker uses; choosing one moves the cursor to its line.
//! The tree-sitter parse runs on the background executor; the popup shows a reading row meanwhile.

mod symbols;

use gpui_kit::{Bounds, EntityId, Subscription, point};
use navo_core::shell_menu::{Group, ItemId, MenuItem, ShellMenu};

use super::editor::EditorBody;
use crate::context_menu::{ContextMenu, ContextMenuEvent, ContextMenuInput};
use crate::prelude::*;
use crate::tokens;
use crate::window_view::NavoWindow;
use symbols::{Symbol, has_grammar, symbols};

/// A finished parse: the popup it was started for, the editor text revision it read, and the
/// symbols it found.
struct Parsed {
    menu: EntityId,
    revision: u64,
    found: Vec<Symbol>,
}

/// The 1-based line of the row `event` chose, given each row's line; other events give `None`.
fn chosen_line(event: &ContextMenuEvent, lines: &[u64]) -> Option<u64> {
    match event {
        ContextMenuEvent::Chosen(ItemId::Shell { command, .. }) => {
            lines.get(*command as usize).copied()
        }
        _ => None,
    }
}

/// The row id of the symbol at `index` (`Shell` ids hold a number and no text).
fn row_id(index: usize) -> ItemId {
    ItemId::Shell {
        command: u32::try_from(index).unwrap_or(u32::MAX),
        verb: None,
    }
}

/// Builds one menu row per symbol: the name, with its line where a shortcut would sit.
fn outline_menu(found: &[Symbol]) -> ShellMenu {
    let items = found
        .iter()
        .enumerate()
        .map(|(index, symbol)| MenuItem {
            text: symbol.name.clone(),
            access_key: None,
            accelerator: Some(symbol.line.to_string()),
            ..MenuItem::new(row_id(index), "", Group::Navo)
        })
        .collect();
    ShellMenu { items }
}

/// The menu shown while the parse runs: one disabled "Reading outline…" row.
fn reading_menu() -> ShellMenu {
    let row = MenuItem {
        text: navo_l10n::tr("viewer-outline-reading"),
        enabled: false,
        ..MenuItem::new(row_id(usize::MAX), "", Group::Navo)
    };
    ShellMenu { items: vec![row] }
}

impl NavoWindow {
    /// Opens the outline of the text editor's file under the viewer header with a reading row
    /// and starts the parse; a file without a grammar gets a note. Other bodies let the key pass.
    pub(in crate::window_view::viewer) fn open_outline(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let t = tokens::current(cx);
        let spacing = (t.spacing.xl, t.spacing.path_height);
        let Some(viewer) = self.viewer.as_mut() else {
            return cx.propagate();
        };
        let Some(body) = viewer.body.as_ref() else {
            return cx.propagate();
        };
        let Some(editor) = body.as_any().downcast_ref::<EditorBody>() else {
            return cx.propagate();
        };
        if !has_grammar(&editor.editor(cx).read(cx).language_name()) {
            viewer.say(navo_l10n::tr("viewer-outline-empty"), cx);
            cx.notify();
            return;
        }
        let area = viewer.bounds.get();
        let input = ContextMenuInput {
            menu: reading_menu(),
            context: navo_l10n::tr("viewer-outline-title").into(),
            anchor: point(area.left() + spacing.0, area.top() + spacing.1),
            bounds: Bounds::new(Point::default(), window.viewport_size()),
            restore: Some(body.focus_handle(cx)),
        };
        let menu = cx.new(|cx| {
            let mut menu = ContextMenu::new(input, window, cx);
            menu.hide_classic(cx);
            menu
        });
        let events = Self::outline_events(&menu, Vec::new(), window, cx);
        viewer.menu = Some((menu.clone(), events));
        viewer.is_view_menu = false;
        self.parse_outline(menu.entity_id(), window, cx);
        cx.notify();
    }

    /// Closes the outline on any choice or dismissal; `lines` maps each row to its line.
    fn outline_events(
        menu: &Entity<ContextMenu>,
        lines: Vec<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.subscribe_in(menu, window, move |this, _, event, window, cx| {
            let chosen = chosen_line(event, &lines);
            if !matches!(event, ContextMenuEvent::OpenSubmenu(_)) {
                this.close_outline(chosen, window, cx);
            }
        })
    }

    /// Parses the editor's current text on the background executor for the popup `menu`,
    /// then hands the symbols to `fill_outline`.
    fn parse_outline(&mut self, menu: EntityId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(body) = self.viewer.as_ref().and_then(|viewer| viewer.body.as_ref()) else {
            return;
        };
        let Some(editor) = body.as_any().downcast_ref::<EditorBody>() else {
            return;
        };
        let revision = body.revision(cx);
        let (language, text) = {
            let state = editor.editor(cx).read(cx);
            (state.language_name().to_string(), state.text().clone())
        };
        cx.spawn_in(window, async move |this, cx| {
            let found = cx
                .background_spawn(async move { symbols(&language, &text.to_string()) })
                .await;
            let parsed = Parsed {
                menu,
                revision,
                found,
            };
            let _ = this.update_in(cx, |view, window, cx| view.fill_outline(parsed, window, cx));
        })
        .detach();
    }

    /// Shows a parse's symbols in its popup. A result for a closed popup is dropped; one for an
    /// older text revision is dropped and the text parsed again; no symbols closes with a note.
    fn fill_outline(&mut self, parsed: Parsed, window: &mut Window, cx: &mut Context<Self>) {
        let Some(viewer) = self.viewer.as_mut() else {
            return;
        };
        let open = viewer.menu.as_ref().map(|(menu, _)| menu.clone());
        let Some(menu) = open.filter(|menu| menu.entity_id() == parsed.menu) else {
            return;
        };
        let Some(body) = viewer.body.as_ref() else {
            return;
        };
        if body.revision(cx) != parsed.revision {
            return self.parse_outline(parsed.menu, window, cx);
        }
        if parsed.found.is_empty() {
            window.focus(&body.focus_handle(cx), cx);
            viewer.menu = None;
            viewer.say(navo_l10n::tr("viewer-outline-empty"), cx);
            cx.notify();
            return;
        }
        let lines = parsed.found.iter().map(|symbol| symbol.line).collect();
        menu.update(cx, |menu, cx| {
            menu.set_menu(outline_menu(&parsed.found), cx)
        });
        let events = Self::outline_events(&menu, lines, window, cx);
        if let Some(viewer) = self.viewer.as_mut() {
            viewer.menu = Some((menu, events));
        }
        cx.notify();
    }

    /// Hands the open popup a result for an older text revision, as a slow parse would (tests).
    #[cfg(test)]
    pub(in crate::window_view::viewer) fn land_stale_outline(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(viewer) = self.viewer.as_ref() else {
            return;
        };
        let (Some((menu, _)), Some(body)) = (viewer.menu.as_ref(), viewer.body.as_ref()) else {
            return;
        };
        let stale = Parsed {
            menu: menu.entity_id(),
            revision: body.revision(cx).wrapping_sub(1),
            found: vec![Symbol {
                name: "stale".into(),
                line: 1,
            }],
        };
        self.fill_outline(stale, window, cx);
    }

    /// Closes the outline; a chosen `line` (1-based) moves the editor's cursor there.
    fn close_outline(&mut self, line: Option<u64>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(viewer) = self.viewer.as_mut() else {
            return;
        };
        viewer.menu = None;
        if let (Some(line), Some(body)) = (line, viewer.body.as_ref()) {
            body.go_to_line(line, window, cx);
        }
        cx.notify();
    }
}
