use rusty_core::PaneId;

use super::FerriteWindow;
use crate::pane_chrome::{DriveMenu, DriveMenuEvent};
use crate::prelude::*;

pub(super) struct MenuState {
    pub(super) pane: PaneId,
    selected: usize,
}

impl FerriteWindow {
    pub(super) fn open_drive_menu(
        &mut self,
        order: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panes = self.panes();
        if let Some(pane) = panes.get(order).or_else(|| panes.last()).copied() {
            self.open_drive_menu_for(pane, window, cx);
        }
    }

    pub(super) fn open_drive_menu_for(
        &mut self,
        pane: PaneId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.drives.list.is_empty() {
            return;
        }
        self.path_edit = None;
        let selected = self.current_drive(pane).unwrap_or(0);
        self.drive_menu = Some(MenuState { pane, selected });
        window.focus(&self.menu_focus, cx);
        cx.notify();
    }

    fn menu_event(&mut self, event: DriveMenuEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = &mut self.drive_menu else {
            return;
        };
        match event {
            DriveMenuEvent::Select(index) => menu.selected = index,
            DriveMenuEvent::Open(index) => {
                let pane = menu.pane;
                self.drive_menu = None;
                self.focus_pane(pane);
                self.pick_drive(index, window, cx);
                self.focus_core_pane(window, cx);
            }
            DriveMenuEvent::Close => {
                self.drive_menu = None;
                self.focus_core_pane(window, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn drive_menu_view(
        &self,
        pane: PaneId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.drive_menu.as_ref().filter(|menu| menu.pane == pane)?;
        let list = self.drives.list.clone();
        let view = DriveMenu::new(("drive-menu", pane.get()), list, menu.selected)
            .current(self.current_drive(pane))
            .busy(self.drives.reconnecting.clone())
            .track_focus(&self.menu_focus)
            .on_event(
                cx.listener(|this: &mut Self, event: &DriveMenuEvent, window, cx| {
                    this.menu_event(*event, window, cx);
                }),
            );
        Some(view.into_popover(px(0.)))
    }
}
