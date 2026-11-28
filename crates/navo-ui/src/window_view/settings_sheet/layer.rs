//! Settings sheet overlay rendering and cloned view snapshots for the floating Settings window.

use std::sync::Arc;

use gpui_kit::{FocusHandle, ScrollHandle};

use super::NavoWindow;
use super::controls::{BottomBar, EditSnapshot, Status, edit_bar};
use super::find::find_bar;
use super::git::git_page_for;
use crate::git::GitHooks;
use crate::prelude::*;
use crate::primitives::TextFieldState;
use crate::settings::{
    DiagnosticRow, Filter, IndexProgress, Moved, RollbackState, SearchPage, SettingsBody,
    SheetEvent, Values,
};
use crate::shortcuts::ShortcutsPage;
use crate::tokens::{self, Theme};
use crate::window_view::UpdateView;
use crate::window_view::settings_window::SettingsRoute;

/// Cloned view-model snapshot of `NavoWindow`'s settings state so `SettingsWindow::render` can
/// draw without reading or updating `Entity<NavoWindow>` during its own render pass.
pub(in crate::window_view) struct SettingsSnapshot {
    values: Values,
    row: usize,
    diagnostics: Vec<DiagnosticRow>,
    diagnostics_open: bool,
    selected_diagnostic: Option<usize>,
    version: SharedString,
    update: UpdateView,
    restart_offered: bool,
    rollback: RollbackState,
    summon_key_taken: bool,
    clipboard_key_taken: bool,
    error: Option<SharedString>,
    service_notice: Option<SharedString>,
    filter: Filter,
    find_input: Option<Entity<TextFieldState>>,
    upgrade: Vec<Moved>,
    index_progress: IndexProgress,
    git_hooks: Option<Arc<dyn GitHooks>>,
    search_page: SearchPage,
    edit: Option<EditSnapshot>,
    status: Option<Status>,
    can_undo: bool,
    shortcuts: Option<Entity<ShortcutsPage>>,
    focus: FocusHandle,
    scroll: ScrollHandle,
}

impl SettingsSnapshot {
    /// Builds a `SettingsBody` from this snapshot with user actions routed through `route` and `on_event`.
    pub(in crate::window_view) fn build_body(
        &self,
        route: &SettingsRoute,
        on_event: impl Fn(&SheetEvent, &mut Window, &mut App) + 'static,
        t: &Theme,
    ) -> SettingsBody {
        let find = self
            .find_input
            .as_ref()
            .map(|input| find_bar(route, input, t));
        let bottom = BottomBar {
            edit: self.edit.as_ref(),
            status: self.status,
            can_undo: self.can_undo,
            values: &self.values,
        };
        SettingsBody::new(self.values.clone(), self.row)
            .diagnostics(self.diagnostics.clone())
            .diagnostics_open(self.diagnostics_open)
            .selected_diagnostic(self.selected_diagnostic)
            .version(self.version.clone())
            .update(self.update.clone(), self.restart_offered)
            .rollback(self.rollback.clone())
            .summon_key_taken(self.summon_key_taken)
            .clipboard_key_taken(self.clipboard_key_taken)
            .messages(self.error.clone(), self.service_notice.clone())
            .filter(self.filter.clone())
            .find(find)
            .upgrade(self.upgrade.clone())
            .index_progress(self.index_progress.clone())
            .git_page(git_page_for(self.git_hooks.as_ref(), &self.values))
            .search_page(self.search_page)
            .editor(edit_bar(route, &bottom, t))
            .shortcuts(
                self.shortcuts
                    .as_ref()
                    .map(|page| page.clone().into_any_element()),
            )
            .track_focus(&self.focus)
            .track_scroll(&self.scroll)
            .on_event(on_event)
    }
}

impl NavoWindow {
    /// Captures a cloneable snapshot of the current Settings UI state.
    pub(in crate::window_view) fn settings_snapshot(&self) -> SettingsSnapshot {
        let error = self.saver.error.clone().or(self.settings.notice.clone());
        let status = self.settings.status.filter(|_| self.saver.error.is_none());
        SettingsSnapshot {
            values: self.values.clone(),
            row: self.settings.row,
            diagnostics: self.settings.diagnostics.clone(),
            diagnostics_open: self.settings.diagnostics_open,
            selected_diagnostic: self.settings.selected_diagnostic,
            version: self.identity.version.clone(),
            update: self.settings.updates.view.clone(),
            restart_offered: self.restart_offered(),
            rollback: self.settings.updates.rollback_state(),
            summon_key_taken: self.summon_key_taken(),
            clipboard_key_taken: self.clipboard_key_taken(),
            error,
            service_notice: self.settings.service_notice.clone(),
            filter: self.settings_filter(),
            find_input: self.settings.find.as_ref().map(|find| find.input.clone()),
            upgrade: self.settings.upgrade.items(),
            index_progress: self.settings.index_progress.clone(),
            git_hooks: self.git_badges.hooks(),
            search_page: SearchPage {
                status: self.settings.service_status,
                is_windows: self.settings.is_windows,
                is_cleared: self.settings.is_history_cleared,
            },
            edit: self.settings.edit.as_ref().map(EditSnapshot::from_edit),
            status,
            can_undo: !self.settings.undo.is_empty(),
            shortcuts: self.shown_shortcuts().cloned(),
            focus: self.settings.focus.clone(),
            scroll: self.settings.scroll.clone(),
        }
    }

    /// Builds the settings sheet overlay; returns `None` when the Settings window flag is on,
    /// while a viewer is open, or while the sheet is hidden.
    pub(in crate::window_view) fn settings_layer(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if super::super::settings_window::on()
            || self.viewer.is_some()
            || !self.settings.sheet.read(cx).is_shown(cx)
        {
            return None;
        }
        let t = tokens::current(cx).clone();
        let frame = self.sheet_frame(&t, window, cx).covering_passive_card();
        let this = cx.weak_entity();
        let on_event = cx.listener(|view, event: &SheetEvent, window, cx| {
            view.settings_event(event.clone(), window, cx);
        });
        let route = self.settings_route(cx);
        let body = self.settings_snapshot().build_body(&route, on_event, &t);
        let sheet = crate::settings::sheet(&self.settings.sheet, frame, body).close(
            "escape",
            move |window, cx| {
                let _ = this.update(cx, |view, cx| view.close_settings(window, cx));
            },
        );
        Some(sheet.into_any_element())
    }
}
