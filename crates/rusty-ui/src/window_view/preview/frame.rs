use crate::prelude::*;
use crate::side_sheet::{SheetFrame, Side};
use crate::tokens::Theme;
use crate::window_view::FerriteWindow;

impl FerriteWindow {
    pub(in crate::window_view) fn sheet_frame(
        &self,
        t: &Theme,
        window: &Window,
        cx: &App,
    ) -> SheetFrame {
        let viewport = window.viewport_size().width;
        let body = viewport - t.spacing.md * 2.;
        let split = viewport - t.spacing.xs * 2.;
        let ratio = self.split.read(cx).ratio();
        let focused = self.browser.state().focused_pane();
        let panes = self.panes();
        let (active, passive_share) = match panes.iter().position(|pane| *pane == focused) {
            Some(1) => (Side::Right, ratio),
            _ if panes.len() < 2 => (Side::Left, 0.5),
            _ => (Side::Left, 1. - ratio),
        };
        let passive_card = split * passive_share - t.spacing.xs * 2.;
        SheetFrame::new(body, passive_card, active)
    }
}
