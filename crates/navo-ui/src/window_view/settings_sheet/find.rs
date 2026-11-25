use gpui_kit::component::input::InputEvent;
use gpui_kit::{Focusable, Subscription};

use crate::prelude::*;
use crate::primitives::{TextFieldState, text_field_state};
use crate::settings::{
    ClearFind, FIND_CONTEXT, Filter, FindToList, Section, child_of, find_field, section_of,
};
use crate::tokens::Theme;
use crate::window_view::NavoWindow;

const PLACEHOLDER: &str = "Find a setting by name, key or help";

pub(super) struct Find {
    input: Entity<TextFieldState>,
    _change: Subscription,
}

impl NavoWindow {
    pub(super) fn prepare_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.find.is_some() {
            return;
        }
        let input = text_field_state(PLACEHOLDER, window, cx);
        let change = cx.subscribe_in(&input, window, |view, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                view.find_changed(cx);
            }
        });
        self.settings.find = Some(Find {
            input,
            _change: change,
        });
    }

    pub(super) fn focus_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.prepare_find(window, cx);
        if let Some(find) = &self.settings.find {
            window.focus(&find.input.read(cx).focus_handle(cx), cx);
        }
    }

    fn find_changed(&mut self, cx: &mut Context<Self>) {
        let Some(find) = &self.settings.find else {
            return;
        };
        let filter = Filter::new(&find.input.read(cx).value())
            .changed_only(self.settings.filter.is_changed_only())
            .marking(&self.values);
        if filter == self.settings.filter {
            return;
        }
        self.apply_filter(filter, cx);
    }

    /// Toggles the changed-only filter when at least one setting differs from its default.
    pub(super) fn toggle_changed_filter(&mut self, cx: &mut Context<Self>) {
        let next = !self.settings.filter.is_changed_only() && self.values.changed_count() > 0;
        self.set_changed_only(next, cx);
    }

    /// Sets the changed-only filter to `on` and moves selection onto an admitted row if needed.
    pub(super) fn set_changed_only(&mut self, on: bool, cx: &mut Context<Self>) {
        let filter = self
            .settings
            .filter
            .clone()
            .changed_only(on)
            .marking(&self.values);
        self.apply_filter(filter, cx);
    }

    fn apply_filter(&mut self, filter: Filter, cx: &mut Context<Self>) {
        if filter.is_empty() {
            self.settings.folds.reveal(self.settings.row);
        }
        let row = filter
            .clone()
            .hiding(self.values.hidden_row())
            .folding(self.settings.folds)
            .nearest(self.settings.row);
        let from = section_of(self.settings.row);
        self.settings.row = row;
        self.settings.filter = filter;
        if self.settings.is_open && from != Section::Search && section_of(row) == Section::Search {
            self.refresh_index_service(cx);
            self.start_index_progress(cx);
        }
        if let Some(child) = child_of(&self.settings_filter(), row) {
            self.settings.scroll.scroll_to_item(child);
        }
        cx.notify();
    }

    pub(super) fn clear_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(find) = &self.settings.find {
            find.input
                .update(cx, |state, cx| state.set_value(String::new(), window, cx));
        }
        self.find_changed(cx);
    }

    pub(super) fn escape_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.is_shortcuts {
            self.close_settings(window, cx);
            return;
        }
        if self.settings.filter.has_words() {
            self.clear_find(window, cx);
            self.focus_settings(window, cx);
            cx.notify();
            return;
        }
        if self.settings.filter.is_changed_only() {
            self.set_changed_only(false, cx);
            self.focus_settings(window, cx);
            cx.notify();
            return;
        }
        self.close_settings(window, cx);
    }

    pub(super) fn find_bar(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let find = self.settings.find.as_ref()?;
        let bar = find_field(t, &find.input)
            .key_context(FIND_CONTEXT)
            .on_action(cx.listener(|view, _: &ClearFind, window, cx| {
                view.escape_settings(window, cx);
            }))
            .on_action(cx.listener(|view, _: &FindToList, window, cx| {
                view.focus_settings(window, cx);
            }));
        Some(bar.into_any_element())
    }
}
