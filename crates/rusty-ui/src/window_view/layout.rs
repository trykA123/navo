use super::FerriteWindow;
use crate::prelude::*;
use crate::primitives::Splitter;
use crate::tokens;

impl FerriteWindow {
    fn panes_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = tokens::current(cx).clone();
        let panes = self.panes();
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
        self.viewer_view(cx).unwrap_or(body)
    }

    pub(super) fn body_area(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
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
