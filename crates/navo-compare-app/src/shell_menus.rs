//! The sidebar context menus, shown through the shared menu host: the tab menu (CM6b, Close tab
//! and Close other tabs) and the Recent entry menu (B5, the start row menu and its actions).
//! A child module of `shell`, so it reads the shell's private fields.

use gpui_kit::{Context, Div, InteractiveElement as _, Pixels, Point, Window};
use navo_compare_profile::recent::StartRow;
use sidewise_kit::context_menu::ContextMenuEvent;

use super::Shell;
use super::shell_keys::CloseTab;
use crate::menu_host::{MenuAt, MenuLine, show_menu};
use crate::start::OpenSelected;
use crate::start_menu::{CopyPaths, ForgetSelected, copy_row_paths, lines};

gpui_kit::actions!(shell, [CloseOthers]);

/// The two menu lines; Close other tabs is dimmed when `tabs` is one.
fn tab_lines(tabs: usize) -> [MenuLine; 2] {
    [
        MenuLine::Run {
            label: "Close tab".into(),
            action: Box::new(CloseTab),
            enabled: true,
        },
        MenuLine::Run {
            label: "Close other tabs".into(),
            action: Box::new(CloseOthers),
            enabled: tabs > 1,
        },
    ]
}

impl Shell {
    /// Closes every tab but the active one, last first, each through `close_tab`.
    pub(super) fn close_others(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for index in (0..self.tabs.len()).rev() {
            if index != self.active {
                self.close_tab(index, window, cx);
            }
        }
    }

    /// Opens the tab menu at `anchor`; the chosen action runs on the focus held now (the tab was
    /// just selected). With nothing focused no menu opens.
    pub(super) fn show_tab_menu(
        &mut self,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(focus) = window.focused(cx) else {
            return;
        };
        let at = MenuAt {
            anchor,
            focus,
            title: "Tab",
        };
        let lines = tab_lines(self.tabs.len());
        show_menu(self, (&lines, at, |shell| &mut shell.menu), window, cx);
    }
}

impl Shell {
    /// Opens the start row menu of sidebar Recent entry `index` at `anchor`. The chosen line runs
    /// on the shell's own focus, so a start list's selection is not touched. No row: no menu.
    pub(super) fn show_recent_menu(
        &mut self,
        (index, anchor): (usize, Point<Pixels>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.sidebar.read(cx).rows().recent.get(index).cloned() else {
            return;
        };
        let lines = lines(&row);
        let at = MenuAt {
            anchor,
            focus: self.handle.clone(),
            title: "Recent row",
        };
        show_menu(self, (&lines, at, |shell| &mut shell.menu), window, cx);
        let Some((menu, _)) = &self.menu else {
            return;
        };
        let closed = cx.subscribe_in(&menu.clone(), window, |_, _, event, window, cx| {
            if matches!(event, ContextMenuEvent::OpenSubmenu(_)) {
                return;
            }
            // Deferred after the host's deferred dispatch, so a chosen line still sees the row.
            let shell = cx.weak_entity();
            window.defer(cx, move |_, cx| {
                let _ = shell.update(cx, |shell, _| shell.menu_row = None);
            });
        });
        self.menu_row = Some((row, closed));
    }

    /// Takes the row of the open Recent menu; `None` when no such menu is open.
    fn take_menu_row(&mut self) -> Option<StartRow> {
        self.menu_row.take().map(|(row, _)| row)
    }

    /// Opens the menu row's session in the active start tab, else in a new tab.
    fn open_menu_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.take_menu_row() else {
            return;
        };
        if self.active_screen().is_none() {
            self.new_tab(window, cx);
        }
        self.reopen((self.active, &row.id), window, cx);
    }
}

/// Binds the start row actions on the shell root for the sidebar Recent menu; each takes the
/// menu row, so after that menu closes (chosen or dismissed) they do nothing.
pub(super) fn recent_actions(root: Div, cx: &mut Context<Shell>) -> Div {
    root.on_action(cx.listener(|this, _: &OpenSelected, window, cx| {
        this.open_menu_row(window, cx);
    }))
    .on_action(cx.listener(|this, _: &CopyPaths, _, cx| {
        if let Some(row) = this.take_menu_row() {
            copy_row_paths(&row, cx);
        }
    }))
    .on_action(cx.listener(|this, _: &ForgetSelected, _, cx| {
        if let Some(row) = this.take_menu_row().filter(|row| row.name.is_none()) {
            this.forget_row(&row.id, cx);
        }
    }))
}

#[cfg(test)]
#[path = "shell_menu_tests.rs"]
mod menu_tests;
