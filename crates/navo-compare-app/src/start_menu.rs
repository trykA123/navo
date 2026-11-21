//! The context menu of a start row: Open, Copy paths, Remove from recent (NC136).
//! Part of the start screen; `start_rows` opens it on a right click, `Shift+F10` or the list key.
//! The shell reuses its lines and copy for the sidebar's Recent entries (B5).

use gpui_kit::{App, Bounds, ClipboardItem, Pixels, Point};
use navo_compare_profile::recent::StartRow;
use sidewise_kit::prelude::*;

use crate::layers::toast;
use crate::menu_host::{MenuAt, MenuLine, show_menu};
use crate::start::{OpenSelected, StartEvent, StartScreen};
use crate::start_rows::row_at;

gpui_kit::actions!(start, [CopyPaths, ForgetSelected, ShowRowMenu]);

/// The three lines of a row's menu; "Remove from recent" is dimmed for a named row.
pub(crate) fn lines(row: &StartRow) -> [MenuLine; 3] {
    let run = |label: &str, action: Box<dyn gpui_kit::Action>, enabled| MenuLine::Run {
        label: label.to_owned().into(),
        action,
        enabled,
    };
    [
        run("Open", Box::new(OpenSelected), true),
        run("Copy paths", Box::new(CopyPaths), true),
        run(
            "Remove from recent",
            Box::new(ForgetSelected),
            row.name.is_none(),
        ),
    ]
}

impl StartScreen {
    /// Opens the menu of the selected row at `anchor`; no selection does nothing.
    pub(crate) fn show_row_menu(
        &mut self,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.selected.and_then(|i| row_at(&self.rows, i)) else {
            return;
        };
        let lines = lines(row);
        let at = MenuAt {
            anchor,
            focus: self.list_focus.clone(),
            title: "Start row",
        };
        show_menu(self, (&lines, at, |this| &mut this.menu), window, cx);
    }

    /// Opens the menu of the selected row below its left edge (the `Shift+F10` key).
    pub(crate) fn show_row_menu_at_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let anchor = self.row_bounds.get().map_or_else(
            || window.mouse_position(),
            |b: Bounds<Pixels>| b.bottom_left(),
        );
        self.show_row_menu(anchor, window, cx);
    }

    /// Copies the selected row's two sides, one per line, to the clipboard, with a toast.
    pub(crate) fn copy_paths(&mut self, cx: &mut Context<Self>) {
        if let Some(row) = self.selected.and_then(|i| row_at(&self.rows, i)) {
            copy_row_paths(row, cx);
        }
    }

    /// Asks the shell to remove the selected row from Recent; a named row is left alone.
    pub(crate) fn forget_selected(&mut self, cx: &mut Context<Self>) {
        let id = self
            .selected
            .and_then(|i| row_at(&self.rows, i))
            .filter(|row| row.name.is_none())
            .map(|row| row.id.clone());
        if let Some(id) = id {
            cx.emit(StartEvent::Forget { id });
        }
    }
}

/// Copies `row`'s two sides, one per line, to the clipboard and toasts "Copied paths".
pub(crate) fn copy_row_paths(row: &StartRow, cx: &mut App) {
    let text = format!("{}\n{}", row.left, row.right);
    cx.write_to_clipboard(ClipboardItem::new_string(text));
    toast::notify("Copied paths", cx);
}
