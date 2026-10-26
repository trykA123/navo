use super::FerriteWindow;
use crate::prelude::*;
use crate::primitives::Splitter;
use crate::tokens;
use rusty_core::PaneId;

pub(super) const ONE_PANE_BELOW: Pixels = px(900.);

impl FerriteWindow {
    pub(super) fn shown_panes(&self, window: &Window) -> Vec<PaneId> {
        let panes = self.panes();
        if panes.len() < 2 || window.viewport_size().width >= ONE_PANE_BELOW {
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
        let t = tokens::current(cx).clone();
        let panes = self.shown_panes(window);
        let mut views: Vec<AnyElement> = panes
            .iter()
            .map(|pane| {
                let card = self.pane_view(*pane, window, cx);
                div()
                    .size_full()
                    .p(t.spacing.xs)
                    .child(card)
                    .into_any_element()
            })
            .collect();
        let body = match (views.pop(), views.pop()) {
            (Some(second), Some(first)) => Splitter::new("panes", &self.split, first, second)
                .line_on_hover()
                .into_any_element(),
            (Some(only), None) => only,
            _ => div().into_any_element(),
        };
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
            .children(self.activity_drawer_layer(window, cx))
    }
}
