use rusty_core::PaneId;

use super::FerriteWindow;
use super::pane::title_of;
use crate::pane_chrome::{PaneMarks, marked_header, marker_tag};
use crate::prelude::*;
use crate::settings::WindowParts;
use crate::tokens;

impl FerriteWindow {
    pub(super) fn pane_marks(&self, pane: PaneId, window: &Window) -> PaneMarks {
        let number = self
            .tiles_layout()
            .numbered()
            .iter()
            .position(|p| *p == pane);
        let is_active = self.browser.state().focused_pane() == pane;
        let is_target = self.browser.target_pane() == Some(pane);
        let shown = self.shown_panes(window).len();
        PaneMarks::new(shown, number.map_or(0, |n| n + 1), is_active, is_target)
    }

    pub(super) fn pane_header(
        &self,
        pane: PaneId,
        marks: PaneMarks,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let t = tokens::current(cx).clone();
        let is_active = self.browser.state().focused_pane() == pane;
        let parts = WindowParts::of(&self.values);
        let (titles, active_index): (Vec<SharedString>, usize) = self
            .browser
            .state()
            .pane(pane)
            .map(|p| (p.tabs().iter().map(title_of).collect(), p.active_index()))
            .unwrap_or_default();
        let shows_tabs = parts.tabs_with_one || titles.len() > 1;
        let drive = self.drive_pill(pane, parts.drive_button, cx);
        if !shows_tabs && !parts.drive_button && marks.number.is_none() {
            let is_menu_open = self.drive_menu.is_some();
            return is_menu_open.then(|| div().relative().h_0().flex_none().child(drive));
        }
        let menu = self.chrome.get(pane).map(|chrome| chrome.tabs.clone());
        let tabs = shows_tabs.then(|| {
            let tabs = Self::tabs(pane, titles, active_index, cx)
                .highlighted(is_active)
                .on_accent(marks.is_filled)
                .when_some(menu, |tabs, menu| tabs.menu(menu));
            div().flex_1().min_w_0().child(tabs)
        });
        let drive = drive.when(marks.is_filled && parts.drive_button, |drive| {
            drive.rounded(t.radii.md).bg(t.surfaces.card)
        });
        let header = marked_header(&t, pane.get(), marks)
            .children(tabs)
            .when(!shows_tabs, |header| header.child(div().flex_1()))
            .children(marker_tag(&t, pane.get(), marks));
        Some(header.child(drive))
    }
}
