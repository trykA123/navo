//! Settings sheet panel state, actions and overlay rendering.

mod change;
mod controls;
mod edit;
mod find;
mod git;
mod history;
mod index_progress;
mod index_upgrade;
mod layer;
mod locate;
mod service;
mod shortcuts;
mod undo;
mod updates;
mod upgrade;

use std::sync::{Arc, Mutex, PoisonError};

use gpui_kit::{FocusHandle, Focusable, ScrollHandle};
use navo_core::Location;

use super::settings_news::SettingsInbox;
use super::settings_save::{SaveReport, SaveSettings, Saver};
use super::settings_window::SettingsHost;
use super::{NavoWindow, SearchHooks};
use crate::prelude::*;
use crate::settings::{
    Change, DIAGNOSTICS_ROW, Field, Filter, Folds, HELP_ROW, Section, ServiceStatus, SheetEvent,
    Values, child_of, clamped_row, outer_row, section_of,
};
use crate::side_sheet::SheetState;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KeyLine {
    pub line: u64,
    pub is_missing: bool,
}

pub type FindLine = Arc<dyn Fn(&str) -> Option<KeyLine> + Send + Sync>;

pub struct SettingsHooks {
    pub values: Values,
    pub load_error: Option<SharedString>,
    pub save: SaveSettings,
    pub file: Option<Location>,
    pub data_folder: Option<Location>,
    pub line_of: FindLine,
    pub news: Option<SettingsInbox>,
    pub disk_use: Arc<dyn Fn() -> Vec<crate::settings::DiagnosticRow> + Send + Sync>,
    pub save_diagnostic_report: Arc<dyn Fn() -> Result<std::path::PathBuf, String> + Send + Sync>,
    pub take_guide_hint: Arc<dyn Fn() -> bool + Send + Sync>,
    pub edit_layout: super::EditLayout,
    pub updates: super::UpdateHooks,
}

impl SettingsHooks {
    pub fn none() -> Self {
        let session = Arc::new(Mutex::new(Values::default()));
        Self {
            values: Values::default(),
            load_error: None,
            save: Arc::new(move |changes| {
                let mut values = session.lock().unwrap_or_else(PoisonError::into_inner);
                *values = changes
                    .iter()
                    .fold(values.clone(), |all, one| all.with(one));
                SaveReport::saved(values.clone())
            }),
            file: None,
            data_folder: None,
            line_of: Arc::new(|_| None),
            news: None,
            disk_use: Arc::new(Vec::new),
            save_diagnostic_report: Arc::new(|| Err(navo_l10n::tr("diagnostics-unavailable"))),
            take_guide_hint: Arc::new(|| false),
            edit_layout: super::no_layout_edits(),
            updates: super::UpdateHooks::none(),
        }
    }
}

pub(super) struct SettingsPanel {
    pub(in crate::window_view) host: SettingsHost,
    sheet: Entity<SheetState>,
    focus: FocusHandle,
    scroll: ScrollHandle,
    is_open: bool,
    row: usize,
    filter: Filter,
    folds: Folds,
    find: Option<find::Find>,
    upgrade: upgrade::Upgrade,
    undo: undo::UndoStack,
    edit: Option<edit::TextEdit>,
    status: Option<controls::Status>,
    restore_preview: bool,
    notice: Option<SharedString>,
    service_notice: Option<SharedString>,
    waiting_locate: Option<Field>,
    file: Option<Location>,
    data_folder: Option<Location>,
    line_of: FindLine,
    shortcuts: Option<shortcuts::ShortcutsPanel>,
    is_shortcuts: bool,
    search: SearchHooks,
    is_windows: bool,
    is_history_cleared: bool,
    service_status: ServiceStatus,
    index_progress: crate::settings::IndexProgress,
    index_poll: index_progress::Poll,
    service_generation: u64,
    service_action_pending: bool,
    index_upgrade_due: bool,
    pub(super) disk_use: Arc<dyn Fn() -> Vec<crate::settings::DiagnosticRow> + Send + Sync>,
    #[cfg(feature = "tester")]
    pub(super) save_diagnostic_report:
        Arc<dyn Fn() -> Result<std::path::PathBuf, String> + Send + Sync>,
    pub(super) diagnostics: Vec<crate::settings::DiagnosticRow>,
    pub(super) diagnostics_open: bool,
    pub(super) selected_diagnostic: Option<usize>,
    pub(super) diagnostics_generation: u64,
    #[cfg(feature = "tester")]
    pub(super) is_saving_report: bool,
    pub(super) take_guide_hint: Arc<dyn Fn() -> bool + Send + Sync>,
    pub(super) news: Option<SettingsInbox>,
    pub(super) updates: updates::Updates,
}

impl SettingsPanel {
    pub(super) fn split(
        hooks: SettingsHooks,
        search: &SearchHooks,
        cx: &mut App,
    ) -> (Values, Saver, Self) {
        let saver = Saver::new(hooks.save, hooks.load_error, hooks.values.clone())
            .editing_layout(hooks.edit_layout);
        (
            hooks.values,
            saver,
            Self {
                news: hooks.news,
                updates: updates::Updates::new(hooks.updates),
                disk_use: hooks.disk_use,
                data_folder: hooks.data_folder,
                #[cfg(feature = "tester")]
                save_diagnostic_report: hooks.save_diagnostic_report,
                take_guide_hint: hooks.take_guide_hint,
                search: search.clone(),
                ..Self::new(hooks.file, hooks.line_of, cx)
            },
        )
    }

    fn new(file: Option<Location>, line_of: FindLine, cx: &mut App) -> Self {
        Self {
            host: SettingsHost::default(),
            sheet: cx.new(|_| SheetState::new()),
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            is_open: false,
            row: 0,
            filter: Filter::default(),
            folds: Folds::default(),
            find: None,
            upgrade: upgrade::Upgrade::default(),
            undo: undo::UndoStack::default(),
            edit: None,
            status: None,
            restore_preview: false,
            notice: None,
            service_notice: None,
            waiting_locate: None,
            file,
            data_folder: None,
            line_of,
            shortcuts: None,
            is_shortcuts: false,
            search: SearchHooks::none(),
            is_windows: cfg!(windows),
            is_history_cleared: false,
            service_status: ServiceStatus::NotAvailable,
            index_progress: crate::settings::IndexProgress::default(),
            index_poll: None,
            service_generation: 0,
            service_action_pending: false,
            index_upgrade_due: false,
            news: None,
            updates: updates::Updates::new(super::UpdateHooks::none()),
            disk_use: Arc::new(Vec::new),
            #[cfg(feature = "tester")]
            save_diagnostic_report: SettingsHooks::none().save_diagnostic_report,
            take_guide_hint: SettingsHooks::none().take_guide_hint,
            diagnostics: Vec::new(),
            diagnostics_open: false,
            selected_diagnostic: None,
            diagnostics_generation: 0,
            #[cfg(feature = "tester")]
            is_saving_report: false,
        }
    }

    pub(super) fn is_open(&self) -> bool {
        self.is_open
    }

    pub(super) fn undo_sent(&mut self, fields: &[Field]) {
        self.undo.sent(fields);
    }

    pub(super) fn undo_learned(&mut self, report: &SaveReport, sent: &[Field]) {
        self.undo.learned(report, sent);
    }
}

impl NavoWindow {
    pub(super) fn toggle_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.viewer.is_some() || self.restart_applying() {
            return;
        }
        if self.settings.is_open {
            self.close_settings(window, cx);
        } else {
            self.open_settings(window, cx);
        }
    }

    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let is_busy = self.prompt.is_some()
            || self.browser.pending().is_some()
            || self.is_multi_rename_open();
        if self.settings.is_open || is_busy || self.viewer.is_some() {
            return;
        }
        self.close_command_center(window, cx);
        self.close_recently_deleted(window, cx);
        if self.app_menu.is_some() {
            self.close_app_menu(window, cx);
        }
        self.drive_menu = None;
        self.path_edit = None;
        if self.preview.is_open() {
            self.close_preview(cx);
            self.settings.restore_preview = true;
        }
        self.browser.dialog_opened();
        self.settings.is_open = true;
        self.refresh_git_tools();
        if section_of(self.settings.row) == Section::Search {
            self.refresh_index_service(cx);
            self.start_index_progress(cx);
        }
        if section_of(self.settings.row) == Section::Diagnostics {
            self.refresh_diagnostics(cx);
        }
        if section_of(self.settings.row) == Section::Help {
            self.refresh_updates(cx);
        }
        self.prepare_find(window, cx);
        self.settings.undo.clear();
        self.settings.notice = None;
        self.settings.status = None;
        self.settings.service_notice = None;
        self.settings.sheet.update(cx, |sheet, cx| sheet.open(cx));
        self.focus_settings(window, cx);
        cx.notify();
    }

    pub(super) fn open_settings_at(
        &mut self,
        field: Field,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_settings(window, cx);
        if self.settings.is_open {
            self.select_setting(field.row(), window, cx);
        }
    }

    fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.is_open = false;
        self.stop_index_progress();
        self.settings.edit = None;
        self.settings.waiting_locate = None;
        self.settings.is_history_cleared = false;
        self.settings.undo.clear();
        self.settings.filter = Filter::default();
        self.clear_find(window, cx);
        self.settings.row = outer_row(self.settings.row);
        self.settings.folds = Folds::default();
        self.settings.diagnostics_open = false;
        self.settings.selected_diagnostic = None;
        self.settings.updates.reset_rollback_ui();
        self.pause_shortcuts(cx);
        self.settings.sheet.update(cx, |sheet, cx| sheet.close(cx));
        if std::mem::take(&mut self.settings.restore_preview) {
            self.open_preview(window, cx);
        }
        self.focus_after_settings(window, cx);
        cx.notify();
    }

    /// Returns focus to an open compare file diff, else to the focused pane.
    fn focus_after_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.host_focus_file(window, cx) {
            self.focus_core_pane(window, cx);
        }
    }

    pub(super) fn focus_settings(&self, window: &mut Window, cx: &mut App) {
        match self.shown_shortcuts() {
            Some(page) => {
                let focus = page.read(cx).focus_handle(cx);
                window.focus(&focus, cx);
            }
            None => window.focus(&self.settings.focus, cx),
        }
    }

    pub(super) fn settings_event(
        &mut self,
        event: SheetEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SheetEvent::Select(row) => self.select_setting(row, window, cx),
            SheetEvent::Fold(row, open) => {
                self.settings.folds.set(row, open);
                self.select_setting(row, window, cx);
            }
            SheetEvent::Shortcuts => self.show_shortcuts(window, cx),
            SheetEvent::Shortcut(command) => self.open_shortcut_from_find(command, window, cx),
            SheetEvent::Set(change) => self.set_from_sheet(change, window, cx),
            SheetEvent::Edit(field) => self.start_edit(field, window, cx),
            SheetEvent::Locate(field) => self.open_settings_file(field, window, cx),
            SheetEvent::Undo => self.undo_setting(window, cx),
            SheetEvent::Find => self.focus_find(window, cx),
            SheetEvent::ToggleChangedFilter => self.toggle_changed_filter(cx),
            SheetEvent::Keep(field) => self.keep_old(field, window, cx),
            SheetEvent::DismissUpgrade => self.dismiss_upgrade(),
            SheetEvent::RefreshDiagnostics => self.refresh_diagnostics(cx),
            SheetEvent::ToggleDiagnosticsFold => self.toggle_diagnostics_fold(),
            SheetEvent::SelectDiagnostic(index) => self.select_diagnostic(index),
            SheetEvent::OpenDataFolder => self.open_data_folder(window, cx),
            SheetEvent::OpenDiagnosticPath(path) => self.open_diagnostic_path(&path, window, cx),
            SheetEvent::CopyDiagnosticPath(path) => self.copy_diagnostic_path(&path, cx),
            SheetEvent::OpenGuide => self.open_guide_from_settings(window, cx),
            SheetEvent::CheckUpdates => self.check_updates_now(cx),
            SheetEvent::RestartUpdate => self.request_restart(window, cx),
            SheetEvent::OpenReleases => cx.open_url(updates::RELEASES_URL),
            SheetEvent::ClearSkippedVersion => self.clear_skipped_version(cx),
            SheetEvent::ToggleRollbackFold => self.toggle_rollback_fold(),
            SheetEvent::PromptRollback(version) => self.prompt_rollback(version),
            SheetEvent::CancelRollback => self.cancel_rollback(),
            SheetEvent::ConfirmRollback(version) => self.confirm_rollback(&version, cx),
            SheetEvent::ClearHistory => self.clear_search_history(),
            SheetEvent::InstallService => self.install_index_service(cx),
            SheetEvent::RemoveService => self.remove_index_service(cx),
            SheetEvent::Close => self.escape_settings(window, cx),
        }
        if self.settings.filter.is_changed_only() {
            self.set_changed_only(self.values.changed_count() > 0, cx);
        } else {
            self.settings.filter = self.settings.filter.clone().marking(&self.values);
        }
        cx.notify();
    }

    fn open_shortcut_from_find(
        &mut self,
        command: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_shortcuts(window, cx);
        if let Some(page) = self.shown_shortcuts().cloned() {
            page.update(cx, |page, cx| {
                page.set_query("", window, cx);
                page.select(command, cx);
            });
        }
    }

    fn toggle_diagnostics_fold(&mut self) {
        self.settings.diagnostics_open = !self.settings.diagnostics_open;
        self.settings.notice = None;
        self.settings.service_notice = None;
        if !self.settings.diagnostics_open {
            self.settings.selected_diagnostic = None;
            if self.settings.row == DIAGNOSTICS_ROW {
                self.settings.row = HELP_ROW;
            }
        }
    }

    fn select_diagnostic(&mut self, index: usize) {
        self.settings.diagnostics_open = true;
        self.settings.selected_diagnostic = Some(index);
        self.settings.row = DIAGNOSTICS_ROW;
        self.settings.notice = None;
        self.settings.service_notice = None;
    }

    fn copy_diagnostic_path(&mut self, path: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(path.to_owned()));
        self.settings.notice = None;
        self.settings.service_notice = Some(navo_l10n::tr("settings-storage-copied").into());
    }
    fn select_setting(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let from = section_of(self.settings.row);
        self.settings.row = clamped_row(row);
        self.settings.selected_diagnostic = None;
        self.settings.folds.reveal(self.settings.row);
        self.settings.notice = None;
        self.settings.status = None;
        self.settings.service_notice = None;
        let was_shortcuts = std::mem::take(&mut self.settings.is_shortcuts);
        if was_shortcuts {
            self.pause_shortcuts(cx);
            self.settings
                .scroll
                .set_offset(gpui_kit::point(px(0.), px(0.)));
        }
        if section_of(self.settings.row) != from {
            self.settings
                .scroll
                .set_offset(gpui_kit::point(px(0.), px(0.)));
        }
        if section_of(self.settings.row) == Section::Search
            && (from != Section::Search || was_shortcuts)
        {
            self.refresh_index_service(cx);
            self.start_index_progress(cx);
        }
        if section_of(self.settings.row) == Section::Diagnostics
            && (from != Section::Diagnostics || was_shortcuts)
        {
            self.refresh_diagnostics(cx);
        }
        if section_of(self.settings.row) == Section::Help
            && (from != Section::Help || was_shortcuts)
        {
            self.refresh_updates(cx);
        }
        if let Some(child) = child_of(&self.settings_filter(), row) {
            self.settings.scroll.scroll_to_item(child);
        }
        if self.settings.edit.is_none() {
            self.focus_settings(window, cx);
        }
    }
}

#[cfg(test)]
mod diagnostics_tests;
#[cfg(test)]
mod test_hooks;
