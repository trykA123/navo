//! The F3 editor's outline popup (Ctrl+Shift+O): the file's symbols in the filterable menu
//! the language picker uses; choosing one moves the cursor to its line.

mod symbols;

use gpui_kit::{Bounds, point};
use navo_core::shell_menu::{Group, ItemId, MenuItem, ShellMenu};

use super::editor::EditorBody;
use crate::context_menu::{ContextMenu, ContextMenuEvent, ContextMenuInput};
use crate::prelude::*;
use crate::tokens;
use crate::window_view::NavoWindow;
use symbols::{Symbol, symbols};

/// The 1-based line of the row `event` chose, given each row's line; other events give `None`.
fn chosen_line(event: &ContextMenuEvent, lines: &[u64]) -> Option<u64> {
    match event {
        ContextMenuEvent::Chosen(ItemId::Shell { command, .. }) => {
            lines.get(*command as usize).copied()
        }
        _ => None,
    }
}

/// Builds one menu row per symbol: the name, with its line where a shortcut would sit.
/// The row id carries the symbol's index (`Shell` ids hold a number and no text).
fn outline_menu(found: &[Symbol]) -> ShellMenu {
    let items = found
        .iter()
        .enumerate()
        .map(|(index, symbol)| {
            let id = ItemId::Shell {
                command: u32::try_from(index).unwrap_or(u32::MAX),
                verb: None,
            };
            MenuItem {
                text: symbol.name.clone(),
                access_key: None,
                accelerator: Some(symbol.line.to_string()),
                ..MenuItem::new(id, "", Group::Navo)
            }
        })
        .collect();
    ShellMenu { items }
}

impl NavoWindow {
    /// Opens the outline of the text editor's file under the viewer header, or notes that
    /// it has no symbols; other bodies let the key pass on.
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
        let found = {
            let entity = editor.editor(cx);
            let state = entity.read(cx);
            symbols(&state.language_name(), &state.text().to_string())
        };
        if found.is_empty() {
            viewer.say(navo_l10n::tr("viewer-outline-empty"), cx);
            cx.notify();
            return;
        }
        let area = viewer.bounds.get();
        let lines: Vec<u64> = found.iter().map(|symbol| symbol.line).collect();
        let input = ContextMenuInput {
            menu: outline_menu(&found),
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
        let events = cx.subscribe_in(&menu, window, move |this, _, event, window, cx| {
            let chosen = chosen_line(event, &lines);
            if !matches!(event, ContextMenuEvent::OpenSubmenu(_)) {
                this.close_outline(chosen, window, cx);
            }
        });
        viewer.menu = Some((menu, events));
        viewer.is_view_menu = false;
        cx.notify();
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
