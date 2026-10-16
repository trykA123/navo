use std::time::Instant;

use gpui_kit::{Bounds, Point};

use super::FerriteWindow;
use super::run::Step;
use crate::context_menu::{ContextMenu, ContextMenuInput};
use crate::prelude::*;

impl FerriteWindow {
    pub(super) fn show_when_ready(
        &mut self,
        (id, step, now): (u64, &mut Step, Instant),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(run) = self.menu_run.as_mut().filter(|run| run.id == id) else {
            return;
        };
        let is_due = now >= run.show.as_ref().map_or(now, |show| show.until);
        let ready = step.menu.is_some() || !run.is_opening();
        if !(ready || is_due) {
            return;
        }
        let Some(show) = run.show.take() else {
            return;
        };
        let menu = step.menu.take().unwrap_or_else(|| run.first_menu());
        let input = ContextMenuInput {
            menu,
            context: show.context,
            anchor: show.anchor,
            bounds: Bounds::new(Point::default(), window.viewport_size()),
            restore: Some(show.restore),
        };
        let has_classic = show.has_classic;
        let view = cx.new(|cx| {
            let mut menu = ContextMenu::new(input, window, cx);
            if !has_classic {
                menu.hide_classic(cx);
            }
            menu
        });
        let events = cx.subscribe_in(&view, window, |this, _, event, window, cx| {
            this.shell_menu_event(event, window, cx);
        });
        run.view = Some((view, events));
        cx.notify();
    }
}
