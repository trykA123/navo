//! Settings sheet overlay rendering: builds the sheet body from the panel state.

use super::NavoWindow;
use crate::prelude::*;
use crate::settings::{SearchPage, SettingsBody, SheetEvent};
use crate::tokens;

impl NavoWindow {
    /// Builds the settings sheet overlay; returns `None` while a viewer is open or the sheet is hidden.
    pub(in crate::window_view) fn settings_layer(
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
            .update(self.settings.updates.view.clone(), self.restart_offered())
            .summon_key_taken(self.summon_key_taken())
            .clipboard_key_taken(self.clipboard_key_taken())
            .messages(error, self.settings.service_notice.clone())
            .filter(self.settings_filter())
            .find(self.find_bar(&t, cx))
            .upgrade(self.settings.upgrade.items())
            .index_progress(self.settings.index_progress.clone())
            .git_page(self.git_page())
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
        Some(sheet.into_any_element())
    }
}
