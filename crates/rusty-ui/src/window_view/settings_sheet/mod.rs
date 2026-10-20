mod edit;
mod find;
mod locate;
mod shortcuts;
mod undo;
mod upgrade;

use std::sync::{Arc, Mutex, PoisonError};

use gpui_kit::{FocusHandle, Focusable, ScrollHandle};
use rusty_core::Location;

use super::FerriteWindow;
use super::settings_news::SettingsInbox;
use super::settings_save::{SaveReport, SaveSettings, Saver};
use crate::prelude::*;
use crate::settings::{Change, FIELDS, Field, Filter, SettingsBody, SheetEvent, Values, child_of};
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
    pub(super) news: Option<SettingsInbox>,
}

impl SettingsPanel {
    pub(super) fn split(hooks: SettingsHooks, cx: &mut App) -> (Values, Saver, Self) {
        let saver = Saver::new(hooks.save, hooks.load_error, hooks.values.clone());
        (
            hooks.values,
            saver,
            Self {
                news: hooks.news,
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
            news: None,
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
        let is_busy = self.prompt.is_some() || self.browser.pending().is_some();
        if self.settings.is_open || is_busy || self.viewer.is_some() {
            return;
        }
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
            SheetEvent::Close => self.escape_settings(window, cx),
        }
        cx.notify();
    }

    fn select_setting(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.row = row.min(FIELDS.len() - 1);
        self.settings.notice = None;
        if std::mem::take(&mut self.settings.is_shortcuts) {
            self.pause_shortcuts(cx);
        }
        if let Some(child) = child_of(&self.settings.filter, row) {
            self.settings.scroll.scroll_to_item(child);
        }
        if self.settings.edit.is_none() {
            self.focus_settings(window, cx);
        }
    }

    fn set_from_sheet(&mut self, change: Change, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(row) = FIELDS.iter().position(|spec| spec.field == change.field) {
            self.select_setting(row, window, cx);
        }
        let old = self.values.change_of(change.field);
        if old == change {
            return;
        }
        self.settings.undo.push(old);
        self.change_setting(change, window, cx);
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
            .error(error)
            .filter(self.settings.filter.clone())
            .find(self.find_bar(&t, cx))
            .upgrade(self.settings.upgrade.items())
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
    pub(in crate::window_view) fn settings_row(&self) -> usize {
        self.settings.row
    }
}
