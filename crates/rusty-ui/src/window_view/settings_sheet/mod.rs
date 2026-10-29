mod edit;
mod find;
mod index_upgrade;
mod locate;
mod service;
mod shortcuts;
mod undo;
mod upgrade;

use std::sync::{Arc, Mutex, PoisonError};

use gpui_kit::{FocusHandle, Focusable, ScrollHandle};
use rusty_core::Location;

use super::settings_news::SettingsInbox;
use super::settings_save::{SaveReport, SaveSettings, Saver};
use super::{FerriteWindow, SearchHooks};
use crate::prelude::*;
use crate::settings::{
    Change, FIELDS, Field, Filter, HELP_ROW, SearchPage, Section, ServiceStatus, SettingsBody,
    SheetEvent, Values, child_of, section_of,
};
use crate::side_sheet::SheetState;
use crate::tokens;

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
    pub line_of: FindLine,
    pub news: Option<SettingsInbox>,
    pub disk_use: Arc<dyn Fn() -> Vec<crate::settings::DiagnosticRow> + Send + Sync>,
    pub save_diagnostic_report: Arc<dyn Fn() -> Result<std::path::PathBuf, String> + Send + Sync>,
    pub take_guide_hint: Arc<dyn Fn() -> bool + Send + Sync>,
    pub edit_layout: super::EditLayout,
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
            line_of: Arc::new(|_| None),
            news: None,
            disk_use: Arc::new(Vec::new),
            save_diagnostic_report: Arc::new(|| Err(rusty_l10n::tr("diagnostics-unavailable"))),
            take_guide_hint: Arc::new(|| false),
            edit_layout: super::no_layout_edits(),
        }
    }
}

pub(super) struct SettingsPanel {
    sheet: Entity<SheetState>,
    focus: FocusHandle,
    scroll: ScrollHandle,
    is_open: bool,
    row: usize,
    filter: Filter,
    find: Option<find::Find>,
    upgrade: upgrade::Upgrade,
    undo: undo::UndoStack,
    edit: Option<edit::TextEdit>,
    restore_preview: bool,
    notice: Option<SharedString>,
    waiting_locate: Option<Field>,
    file: Option<Location>,
    line_of: FindLine,
    shortcuts: Option<shortcuts::ShortcutsPanel>,
    is_shortcuts: bool,
    search: SearchHooks,
    is_windows: bool,
    is_history_cleared: bool,
    service_status: ServiceStatus,
    service_generation: u64,
    service_action_pending: bool,
    index_upgrade_due: bool,
    pub(super) disk_use: Arc<dyn Fn() -> Vec<crate::settings::DiagnosticRow> + Send + Sync>,
    pub(super) save_diagnostic_report:
        Arc<dyn Fn() -> Result<std::path::PathBuf, String> + Send + Sync>,
    pub(super) diagnostics: Vec<crate::settings::DiagnosticRow>,
    pub(super) diagnostics_generation: u64,
    pub(super) is_saving_report: bool,
    pub(super) take_guide_hint: Arc<dyn Fn() -> bool + Send + Sync>,
    pub(super) news: Option<SettingsInbox>,
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
                disk_use: hooks.disk_use,
                save_diagnostic_report: hooks.save_diagnostic_report,
                take_guide_hint: hooks.take_guide_hint,
                search: search.clone(),
                ..Self::new(hooks.file, hooks.line_of, cx)
            },
        )
    }

    fn new(file: Option<Location>, line_of: FindLine, cx: &mut App) -> Self {
        Self {
            sheet: cx.new(|_| SheetState::new()),
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            is_open: false,
            row: 0,
            filter: Filter::default(),
            find: None,
            upgrade: upgrade::Upgrade::default(),
            undo: undo::UndoStack::default(),
            edit: None,
            restore_preview: false,
            notice: None,
            waiting_locate: None,
            file,
            line_of,
            shortcuts: None,
            is_shortcuts: false,
            search: SearchHooks::none(),
            is_windows: cfg!(windows),
            is_history_cleared: false,
            service_status: ServiceStatus::NotAvailable,
            service_generation: 0,
            service_action_pending: false,
            index_upgrade_due: false,
            news: None,
            disk_use: Arc::new(Vec::new),
            save_diagnostic_report: SettingsHooks::none().save_diagnostic_report,
            take_guide_hint: SettingsHooks::none().take_guide_hint,
            diagnostics: Vec::new(),
            diagnostics_generation: 0,
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

impl FerriteWindow {
    pub(super) fn toggle_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.viewer.is_some() {
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
        self.settings.is_open = true;
        if section_of(self.settings.row) == Section::Search {
            self.refresh_index_service(cx);
        }
        if section_of(self.settings.row) == Section::Diagnostics {
            self.refresh_diagnostics(cx);
        }
        self.prepare_find(window, cx);
        self.settings.undo.clear();
        self.settings.notice = None;
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
        let row = FIELDS.iter().position(|spec| spec.field == field);
        if let Some(row) = row.filter(|_| self.settings.is_open) {
            self.select_setting(row, window, cx);
        }
    }

    fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.is_open = false;
        self.settings.edit = None;
        self.settings.waiting_locate = None;
        self.settings.is_history_cleared = false;
        self.settings.undo.clear();
        self.clear_find(window, cx);
        self.pause_shortcuts(cx);
        self.settings.sheet.update(cx, |sheet, cx| sheet.close(cx));
        if std::mem::take(&mut self.settings.restore_preview) {
            self.open_preview(window, cx);
        }
        self.focus_core_pane(window, cx);
        cx.notify();
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

    fn settings_event(&mut self, event: SheetEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            SheetEvent::Select(row) => self.select_setting(row, window, cx),
            SheetEvent::Shortcuts => self.show_shortcuts(window, cx),
            SheetEvent::Set(change) => self.set_from_sheet(change, window, cx),
            SheetEvent::Edit(field) => self.start_edit(field, window, cx),
            SheetEvent::Locate(field) => self.open_settings_file(field, window, cx),
            SheetEvent::Undo => self.undo_setting(window, cx),
            SheetEvent::Find => self.focus_find(window, cx),
            SheetEvent::Keep(field) => self.keep_old(field, window, cx),
            SheetEvent::DismissUpgrade => self.dismiss_upgrade(),
            SheetEvent::RefreshDiagnostics => self.refresh_diagnostics(cx),
            SheetEvent::OpenGuide => self.open_guide_from_settings(window, cx),
            SheetEvent::ClearHistory => self.clear_search_history(),
            SheetEvent::InstallService => self.install_index_service(cx),
            SheetEvent::RemoveService => self.remove_index_service(cx),
            SheetEvent::Close => self.escape_settings(window, cx),
        }
        cx.notify();
    }

    fn clear_search_history(&mut self) {
        (self.settings.search.clear)();
        self.settings.is_history_cleared = true;
    }

    fn select_setting(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let from = section_of(self.settings.row);
        self.settings.row = row.min(HELP_ROW);
        self.settings.notice = None;
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
        }
        if section_of(self.settings.row) == Section::Diagnostics
            && (from != Section::Diagnostics || was_shortcuts)
        {
            self.refresh_diagnostics(cx);
        }
        if let Some(child) = child_of(&self.settings.filter, row) {
            self.settings.scroll.scroll_to_item(child);
        }
        if self.settings.edit.is_none() {
            self.focus_settings(window, cx);
        }
    }

    fn set_from_sheet(&mut self, change: Change, window: &mut Window, cx: &mut Context<Self>) {
        let field = change.field;
        if self.apply_from_sheet(change, window, cx) {
            self.settings.upgrade.forget(field);
        }
    }

    fn apply_from_sheet(
        &mut self,
        change: Change,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(row) = FIELDS.iter().position(|spec| spec.field == change.field) {
            self.select_setting(row, window, cx);
        }
        let old = self.values.change_of(change.field);
        if old == change {
            return false;
        }
        self.settings.undo.push(old);
        self.change_setting(change, window, cx);
        true
    }

    fn undo_setting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(old) = self.settings.undo.pop() else {
            return;
        };
        if let Some(row) = FIELDS.iter().position(|spec| spec.field == old.field) {
            self.select_setting(row, window, cx);
        }
        self.change_setting(old, window, cx);
    }

    pub(super) fn settings_layer(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.viewer.is_some() || !self.settings.sheet.read(cx).is_shown(cx) {
            return None;
        }
        let t = tokens::current(cx).clone();
        let frame = self.sheet_frame(&t, window, cx).covering_passive_card();
        let this = cx.weak_entity();
        let error = self.saver.error.clone().or(self.settings.notice.clone());
        let body = SettingsBody::new(self.values.clone(), self.settings.row)
            .diagnostics(self.settings.diagnostics.clone())
            .version(self.identity.version.clone())
            .error(error)
            .filter(self.settings.filter.clone())
            .find(self.find_bar(&t, cx))
            .upgrade(self.settings.upgrade.items())
            .search_page(SearchPage {
                status: self.settings.service_status,
                is_windows: self.settings.is_windows,
                is_cleared: self.settings.is_history_cleared,
            })
            .editor(self.edit_bar(&t, cx))
            .shortcuts(
                self.shown_shortcuts()
                    .map(|page| page.clone().into_any_element()),
            )
            .track_focus(&self.settings.focus)
            .track_scroll(&self.settings.scroll)
            .on_event(cx.listener(|view, event: &SheetEvent, window, cx| {
                view.settings_event(event.clone(), window, cx);
            }));
        let sheet = crate::settings::sheet(&self.settings.sheet, frame, body).close(
            "escape",
            move |window, cx| {
                let _ = this.update(cx, |view, cx| view.close_settings(window, cx));
            },
        );
        Some(
            div()
                .absolute()
                .top(t.spacing.xs)
                .bottom(t.spacing.xs)
                .left(t.spacing.md)
                .right(t.spacing.md)
                .child(sheet)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
impl FerriteWindow {
    pub(in crate::window_view) fn scroll_settings_to_end(&self) {
        let end = self.settings.scroll.max_offset().y;
        self.settings
            .scroll
            .set_offset(gpui_kit::point(px(0.), -end));
    }

    pub(in crate::window_view) fn set_settings_windows(&mut self, is_windows: bool) {
        self.settings.is_windows = is_windows;
    }

    pub(in crate::window_view) fn settings_row(&self) -> usize {
        self.settings.row
    }
}

#[cfg(test)]
mod diagnostics_tests;
