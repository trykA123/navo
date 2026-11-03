use std::rc::Rc;

use gpui_kit::{FocusHandle, ScrollHandle};

use super::{
    DiagnosticRow, Filter, GitPage, HELP_ROW, IndexProgress, Moved, SearchPage, SettingsBody,
    SheetEvent, Values,
};
use crate::prelude::*;
use crate::window_view::UpdateView;

impl SettingsBody {
    pub fn new(values: Values, row: usize) -> Self {
        Self {
            filter: Filter::default().hiding(values.hidden_row()),
            values,
            row: row.min(HELP_ROW),
            error: None,
            editor: None,
            shortcuts: None,
            find: None,
            upgrade: Vec::new(),
            page: SearchPage::default(),
            git: GitPage::default(),
            index: IndexProgress::default(),
            diagnostics: Vec::new(),
            version: SharedString::default(),
            update: UpdateView::Hidden,
            restart_offered: false,
            focus: None,
            scroll: None,
            on_event: None,
            summon_key_taken: false,
            clipboard_key_taken: false,
        }
    }

    #[must_use]
    pub fn summon_key_taken(mut self, taken: bool) -> Self {
        self.summon_key_taken = taken;
        self
    }

    #[must_use]
    pub fn clipboard_key_taken(mut self, taken: bool) -> Self {
        self.clipboard_key_taken = taken;
        self
    }

    #[must_use]
    pub fn error(mut self, message: Option<SharedString>) -> Self {
        self.error = message;
        self
    }

    #[must_use]
    pub fn editor(mut self, editor: Option<AnyElement>) -> Self {
        self.editor = editor;
        self
    }

    #[must_use]
    pub fn shortcuts(mut self, page: Option<AnyElement>) -> Self {
        self.shortcuts = page;
        self
    }

    #[must_use]
    pub fn filter(mut self, filter: Filter) -> Self {
        self.filter = filter.hiding(self.values.hidden_row());
        self
    }

    #[must_use]
    pub fn find(mut self, bar: Option<AnyElement>) -> Self {
        self.find = bar;
        self
    }

    #[must_use]
    pub fn upgrade(mut self, items: Vec<Moved>) -> Self {
        self.upgrade = items;
        self
    }

    #[must_use]
    pub fn search_page(mut self, page: SearchPage) -> Self {
        self.page = page;
        self
    }

    #[must_use]
    pub fn git_page(mut self, page: GitPage) -> Self {
        self.git = page;
        self
    }

    #[must_use]
    pub fn index_progress(mut self, progress: IndexProgress) -> Self {
        self.index = progress;
        self
    }

    #[must_use]
    pub fn diagnostics(mut self, rows: Vec<DiagnosticRow>) -> Self {
        self.diagnostics = rows;
        self
    }

    #[must_use]
    pub fn version(mut self, version: SharedString) -> Self {
        self.version = version;
        self
    }

    /// The update state; `restart_offered` shows the Restart button beside a ready update.
    #[must_use]
    pub fn update(mut self, update: UpdateView, restart_offered: bool) -> Self {
        self.update = update;
        self.restart_offered = restart_offered;
        self
    }

    #[must_use]
    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    #[must_use]
    pub fn track_scroll(mut self, scroll: &ScrollHandle) -> Self {
        self.scroll = Some(scroll.clone());
        self
    }

    #[must_use]
    pub fn on_event(
        mut self,
        handler: impl Fn(&SheetEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_event = Some(Rc::new(handler));
        self
    }
}
