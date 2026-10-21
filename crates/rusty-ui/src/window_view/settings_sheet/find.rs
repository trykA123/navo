use gpui_kit::component::input::InputEvent;
use gpui_kit::{Focusable, Subscription};

use crate::prelude::*;
use crate::primitives::{TextFieldState, text_field_state};
use crate::settings::{
    ClearFind, FIND_CONTEXT, Filter, FindToList, Section, child_of, find_field, section_of,
};
use crate::tokens::Theme;
use crate::window_view::FerriteWindow;

const PLACEHOLDER: &str = "Find a setting by name, key or help";

pub(super) struct Find {
    input: Entity<TextFieldState>,
    _change: Subscription,
}

impl FerriteWindow {
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
        let filter = Filter::new(&find.input.read(cx).value());
        if filter == self.settings.filter {
            return;
        }
        let row = filter.nearest(self.settings.row);
        let from = section_of(self.settings.row);
        self.settings.row = row;
        self.settings.filter = filter;
        if self.settings.is_open && from != Section::Search && section_of(row) == Section::Search {
            self.refresh_index_service(cx);
        }
        if let Some(child) = child_of(&self.settings.filter, row) {
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
        if self.settings.filter.is_empty() || self.settings.is_shortcuts {
            self.close_settings(window, cx);
            return;
        }
        self.clear_find(window, cx);
        self.focus_settings(window, cx);
        cx.notify();
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
