use std::rc::Rc;

use gpui_kit::{FocusHandle, ScrollHandle};

use super::{DiagnosticRow, Filter, HELP_ROW, Moved, SearchPage, SettingsBody, SheetEvent, Values};
use crate::prelude::*;

impl SettingsBody {
    pub fn new(values: Values, row: usize) -> Self {
        Self {
            values,
            row: row.min(HELP_ROW),
            error: None,
            editor: None,
            shortcuts: None,
            filter: Filter::default(),
            find: None,
            upgrade: Vec::new(),
            page: SearchPage::default(),
            diagnostics: Vec::new(),
            version: SharedString::default(),
            focus: None,
            scroll: None,
            on_event: None,
            summon_key_taken: false,
        }
    }

    #[must_use]
    pub fn summon_key_taken(mut self, taken: bool) -> Self {
        self.summon_key_taken = taken;
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
        self.filter = filter;
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
    pub fn diagnostics(mut self, rows: Vec<DiagnosticRow>) -> Self {
        self.diagnostics = rows;
        self
    }

    #[must_use]
    pub fn version(mut self, version: SharedString) -> Self {
        self.version = version;
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
