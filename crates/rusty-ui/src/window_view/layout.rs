use super::FerriteWindow;
use crate::prelude::*;
use crate::tokens;
use rusty_core::PaneId;

pub(super) const ONE_PANE_BELOW: Pixels = px(900.);

impl FerriteWindow {
    pub(super) fn shown_panes(&self, window: &Window) -> Vec<PaneId> {
        let panes = self.panes();
        if panes.len() < 2
            || (window.viewport_size().width >= ONE_PANE_BELOW
                && self.tiles.fits(&self.tiles_layout()))
        {
            return panes;
        }
        let shown = self
            .drive_menu
            .as_ref()
            .map(|menu| menu.pane)
            .unwrap_or_else(|| self.browser.state().focused_pane());
        match panes.contains(&shown) {
            true => vec![shown],
            false => panes.into_iter().take(1).collect(),
        }
    }

    fn panes_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let body = self.tiles_view(window, cx);
        self.viewer_view(window, cx).unwrap_or(body)
    }

    pub(super) fn body_area(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        self.end_finished_drag(cx);
        let t = tokens::current(cx).clone();
        let body = self.panes_view(window, cx);
        let has_strip = self.fkey_strip_height(&t) > px(0.);
        div()
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            .px(t.spacing.xs)
            .when(!has_strip, |area| area.pb(t.spacing.md))
            .child(body)
            .children(self.job_card_layer(cx))
            .children(self.preview_layer(window, cx))
            .children(self.settings_layer(window, cx))
            .children(self.hints_layer(window, cx))
            .children(self.workspace_tray(window, cx))
            .children(self.activity_drawer_layer(window, cx))
    }
}
