use std::time::Duration;

use gpui_kit::Task;
use rusty_core::{Command, PaneId};

use super::FerriteWindow;
use super::drag_hover::Over;
use super::drop::DragSession;
use crate::file_list::{DropEvent, DropSpot, ListEdge};
use crate::prelude::*;

const SCROLL_EVERY: Duration = Duration::from_millis(50);
const FOLDER_AFTER: Duration = Duration::from_millis(800);
const TAB_AFTER: Duration = Duration::from_millis(500);
const DRIVE_AFTER: Duration = Duration::from_millis(800);

#[derive(Default)]
pub(super) struct Spring {
    hover: Option<(Over, Task<()>)>,
    scroll: Option<(PaneId, ListEdge, Task<()>)>,
    pub(super) menu: Option<PaneId>,
    landed: Option<PaneId>,
}

impl Spring {
    pub(super) fn stop(&mut self) {
        self.hover = None;
        self.scroll = None;
    }
}

fn scroll_task(
    (pane, edge): (PaneId, ListEdge),
    window: &Window,
    cx: &mut Context<FerriteWindow>,
) -> Task<()> {
    cx.spawn_in(window, async move |this, cx| {
        let mut more = true;
        while more {
            cx.background_executor().timer(SCROLL_EVERY).await;
            more = this
                .update_in(cx, |view, _, cx| {
                    cx.has_active_drag() && view.scroll_tick(pane, edge, cx)
                })
                .unwrap_or(false);
        }
    })
}

impl FerriteWindow {
    pub(super) fn end_drag(&mut self) -> Option<DragSession> {
        let mut session = self.drag.session.take()?;
        session.spring.stop();
        let sprung = session.spring.menu.take();
        if sprung.is_some() && self.drive_menu.as_ref().map(|menu| menu.pane) == sprung {
            self.drive_menu = None;
        }
        Some(session)
    }

    pub(super) fn landed(&self, mut event: DropEvent) -> DropEvent {
        let session = self.drag.session.as_ref();
        if session.is_some_and(|session| session.spring.landed == Some(event.pane)) {
            event.spot = DropSpot::Folder;
        }
        event
    }

    pub(super) fn hovered_pane(&mut self, pane: PaneId) {
        if let Some(session) = self.drag.session.as_mut() {
            session.spring.landed = session.spring.landed.filter(|landed| *landed != pane);
        }
    }

    pub(super) fn arm_spring(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(session) = self.drag.session.as_ref() else {
            return;
        };
        let over = session.over.clone();
        if session.spring.hover.as_ref().map(|(armed, _)| armed) == over.as_ref() {
            return;
        }
        let delay = over
            .as_ref()
            .filter(|_| !cx.reduce_motion() && !session.outgoing)
            .and_then(|over| self.spring_delay(over, session));
        let armed = over.zip(delay).map(|(over, delay)| {
            let task = cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(delay).await;
                let _ = this.update_in(cx, |view, _, cx| view.spring_due(cx));
            });
            (over, task)
        });
        if let Some(session) = self.drag.session.as_mut() {
            session.spring.hover = armed;
        }
    }

    fn spring_delay(&self, over: &Over, session: &DragSession) -> Option<Duration> {
        match over {
            Over::Spot(pane, spot @ DropSpot::Row(..)) => match self.drop_target(*pane, spot)? {
                Ok(target) if !session.carried.sources.contains(&target) => Some(FOLDER_AFTER),
                _ => None,
            },
            Over::Tab(pane, tab) => {
                let active = self.browser.state().pane(*pane)?.active_index();
                (active != *tab).then_some(TAB_AFTER)
            }
            Over::Drive(pane, _) => {
                let open = self.drive_menu.as_ref().map(|menu| menu.pane);
                (open != Some(*pane)).then_some(DRIVE_AFTER)
            }
            _ => None,
        }
    }

    fn spring_due(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.drag.session.as_ref() else {
            return;
        };
        let armed = session.spring.hover.as_ref().map(|(over, _)| over.clone());
        let Some(over) = armed.filter(|armed| session.over.as_ref() == Some(armed)) else {
            return;
        };
        match over {
            Over::Spot(pane, spot) => self.spring_folder(pane, &spot),
            Over::Tab(pane, tab) => self.spring_tab(pane, tab),
            Over::Drive(pane, _) => self.spring_drive_menu(pane),
            Over::Crumb(..) | Over::Menu(..) => {}
        }
        self.drag_list_changed(cx);
        cx.notify();
    }

    fn spring_folder(&mut self, pane: PaneId, spot: &DropSpot) {
        let Some(Ok(location)) = self.drop_target(pane, spot) else {
            return;
        };
        self.in_pane(pane, &Command::PaneNavigate { location });
        if let Some(session) = self.drag.session.as_mut() {
            session.over = Some(Over::Spot(pane, DropSpot::Folder));
            session.shown_target = None;
            session.spring.landed = Some(pane);
        }
    }

    fn spring_tab(&mut self, pane: PaneId, tab: usize) {
        if let Ok(tab) = u32::try_from(tab) {
            self.in_pane(pane, &Command::PaneActivateTab { tab });
        }
    }

    fn in_pane(&mut self, pane: PaneId, command: &Command) {
        self.focus_pane(pane);
        let result = self.browser.command(command);
        self.show_refusal(result);
    }

    pub(super) fn drag_edge(
        &mut self,
        (pane, edge): (PaneId, Option<ListEdge>),
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let reduce = cx.reduce_motion();
        let Some(session) = self.drag.session.as_mut() else {
            return;
        };
        let current = session
            .spring
            .scroll
            .as_ref()
            .map(|(at, edge, _)| (*at, *edge));
        let internal = session.external.is_none() && !session.outgoing;
        match edge {
            None if current.is_some_and(|(at, _)| at == pane) => session.spring.scroll = None,
            Some(edge) if internal && !reduce && current != Some((pane, edge)) => {
                session.spring.scroll = Some((pane, edge, scroll_task((pane, edge), window, cx)));
            }
            _ => {}
        }
    }

    fn next_top(&self, pane: PaneId, edge: ListEdge) -> Option<u64> {
        let state = self.browser.state().pane(pane)?;
        let tab = state.active();
        let len = tab.listing().map_or(0, |listing| listing.len());
        let last = len.saturating_sub(state.page_rows());
        let top = tab.view().scroll;
        match edge {
            ListEdge::Top => top.checked_sub(1),
            ListEdge::Bottom => (top < last).then_some(top + 1),
        }
    }

    fn scroll_tick(&mut self, pane: PaneId, edge: ListEdge, cx: &mut Context<Self>) -> bool {
        let Some(to) = self.next_top(pane, edge) else {
            return false;
        };
        self.focus_pane(pane);
        let _ = self.browser.command(&Command::PaneScroll { to });
        if let Some(session) = self.drag.session.as_mut()
            && matches!(&session.over, Some(Over::Spot(over, DropSpot::Row(..))) if *over == pane)
        {
            session.over = None;
            session.spring.hover = None;
            self.hovered_pane(pane);
        }
        self.drag_list_changed(cx);
        cx.notify();
        true
    }
}
