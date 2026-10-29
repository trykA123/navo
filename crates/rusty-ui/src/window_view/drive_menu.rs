use rusty_core::{Location, PaneId};

use super::FerriteWindow;
use super::drag_hover::Over;
use crate::file_list::DropPayload;
use crate::pane_chrome::{DriveMenu, DriveMenuEvent, DrivePill};
use crate::prelude::*;

type DriveHover = Box<dyn Fn(&Location, bool, &mut Window, &mut App)>;
type DriveDrop = Box<dyn Fn(&Location, &DropPayload, &mut Window, &mut App)>;

pub(super) struct MenuState {
    pub(super) pane: PaneId,
    selected: usize,
}

impl FerriteWindow {
    pub(super) fn wire_drive_drops(
        pill: DrivePill,
        pane: PaneId,
        cx: &mut Context<Self>,
    ) -> DrivePill {
        let (hover, drop) = Self::drive_landing(pane, Over::Drive, cx);
        pill.on_hover(hover).on_drop(drop)
    }

    fn drive_landing(
        pane: PaneId,
        over: fn(PaneId, Location) -> Over,
        cx: &mut Context<Self>,
    ) -> (DriveHover, DriveDrop) {
        let (hover, drop) = (cx.weak_entity(), cx.weak_entity());
        let on_hover = move |root: &Location, inside, window: &mut Window, cx: &mut App| {
            let _ = hover.update(cx, |view, cx| {
                view.place_hover((over(pane, root.clone()), inside), window, cx);
            });
        };
        let on_drop =
            move |root: &Location, payload: &DropPayload, window: &mut Window, cx: &mut App| {
                let _ = drop.update(cx, |view, cx| {
                    view.drop_on((root.clone(), payload), window, cx);
                });
            };
        (Box::new(on_hover), Box::new(on_drop))
    }

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
        self.refresh_drives(cx);
        if self.drives.list.is_empty() {
            return;
        }
        self.path_edit = None;
        let selected = self.current_drive(pane).unwrap_or(0);
        self.drive_menu = Some(MenuState { pane, selected });
        window.focus(&self.menu_focus, cx);
        cx.notify();
    }

    pub(super) fn spring_drive_menu(&mut self, pane: PaneId) {
        let Some(session) = self.drag.session.as_mut() else {
            return;
        };
        let open = self.drive_menu.as_ref().map(|menu| menu.pane);
        if self.drives.list.is_empty() || open.is_some_and(|open| session.spring.menu != Some(open))
        {
            return;
        }
        session.spring.menu = Some(pane);
        let selected = self.current_drive(pane).unwrap_or(0);
        self.drive_menu = Some(MenuState { pane, selected });
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
        let (hover, drop) = Self::drive_landing(pane, Over::Menu, cx);
        Some(view.on_hover(hover).on_drop(drop).into_popover(px(0.)))
    }
}
