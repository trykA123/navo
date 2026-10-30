use crate::prelude::*;

use crate::window_view::FerriteWindow;

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
