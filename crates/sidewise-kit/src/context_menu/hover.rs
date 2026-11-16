use std::time::Duration;

use gpui_kit::{Context, Pixels, Point, Window};

use super::ContextMenu;
use super::aim::is_aiming;
use super::state::Spot;

pub(super) const HOVER_DELAY: Duration = Duration::from_millis(250);

impl ContextMenu {
    /// Hover intent: the row highlights at once, flyouts open or close only after the pointer
    /// rests, and a pointer heading for the open flyout leaves it alone.
    pub(super) fn pointed(&mut self, spot: Spot, position: Point<Pixels>, cx: &mut Context<Self>) {
        let from = self.pointer.replace(position);
        if self.state.is_open_parent(spot) {
            self.intent = None;
            self.settled = Some(spot);
            if self.state.point(spot) {
                cx.notify();
            }
            return;
        }
        if from.is_some_and(|from| self.aims_at_child(spot.level, from, position)) {
            self.settled = None;
            self.schedule(spot, cx);
            return;
        }
        if self.settled == Some(spot) {
            return;
        }
        self.settled = Some(spot);
        self.intent = None;
        if self.state.needs_rest(spot) {
            self.schedule(spot, cx);
        }
        if self.state.point(spot) {
            self.settle(cx);
            cx.notify();
        }
    }

    pub(super) fn pointer_left(&mut self, at: Point<Pixels>) {
        if self
            .frames()
            .iter()
            .any(|placed| placed.frame.bounds.contains(&at))
        {
            return;
        }
        self.pointer = None;
        self.settled = None;
        self.intent = None;
    }

    pub(super) fn clicked(&mut self, spot: Spot, window: &mut Window, cx: &mut Context<Self>) {
        let event = self.state.click(spot);
        self.conclude(event, window, cx);
    }

    fn schedule(&mut self, spot: Spot, cx: &mut Context<Self>) {
        let path = self.state.row_path(spot).map(<[usize]>::to_vec);
        self.intent = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HOVER_DELAY).await;
            let _ = this.update(cx, |this, cx| this.rested(spot, path.as_deref(), cx));
        }));
    }

    fn rested(&mut self, spot: Spot, path: Option<&[usize]>, cx: &mut Context<Self>) {
        self.intent = None;
        if self.state.row_path(spot) != path {
            return;
        }
        self.settled = Some(spot);
        let event = self.state.rest(spot);
        self.settle(cx);
        if let Some(event) = event {
            cx.emit(event);
        }
        cx.notify();
    }

    fn aims_at_child(&self, level: usize, from: Point<Pixels>, to: Point<Pixels>) -> bool {
        if self.state.levels().len() <= level + 1 {
            return false;
        }
        self.frames()
            .get(level + 1)
            .is_some_and(|child| is_aiming(from, to, child.frame.bounds, child.side))
    }
}
