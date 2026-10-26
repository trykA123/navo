use gpui_kit::Modifiers;
use rusty_core::{DropKeys, Location, PaneId, TabId};

use super::FerriteWindow;
use super::drop::{Carried, Target, keys_of, op_for, refusal_of};
use crate::browser::refusal_from;
use crate::drag::{Hover, Refusal};
use crate::file_list::DropSpot;
use crate::pane_chrome::path_segments;
use crate::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Over {
    Spot(PaneId, DropSpot),
    Tab(PaneId, usize),
    Crumb(PaneId, Location),
    Drive(PaneId, Location),
    Menu(PaneId, Location),
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct TabLanding {
    pane: PaneId,
    index: usize,
    id: TabId,
    location: Location,
}

fn label_of(location: &Location) -> SharedString {
    path_segments(location)
        .pop()
        .map(|segment| segment.label)
        .unwrap_or_else(|| location.as_str().to_owned().into())
}

fn hover_of(carried: &Carried, target: Target, keys: DropKeys) -> Hover {
    let (target_label, refusal) = match &target {
        Ok(target) => (
            label_of(target),
            refusal_from(target, (&carried.parents, &carried.folders)).and_then(refusal_of),
        ),
        Err(refused) => (SharedString::default(), Some(*refused)),
    };
    let op = match (&target, refusal) {
        (Ok(target), None) => Some(op_for(&carried.sources, target, keys)),
        _ => None,
    };
    Hover {
        target_label,
        op,
        refusal: refusal.map(Refusal::text),
    }
}

fn restrict_hover(
    mut hover: Option<Hover>,
    session: &super::drop::DragSession,
    blocked: bool,
) -> Option<Hover> {
    let Some(value) = &mut hover else {
        return hover;
    };
    let requested = value.op;
    if let Some(native) = session.native.as_ref().filter(|native| native.right) {
        value.op = value.op.and_then(|op| {
            if native.allows(op) {
                Some(op)
            } else {
                [
                    rusty_core::DropOp::Copy,
                    rusty_core::DropOp::Move,
                    rusty_core::DropOp::Link,
                ]
                .into_iter()
                .find(|op| native.allows(*op))
            }
        });
    }
    let denied = requested.is_some()
        && session
            .native
            .as_ref()
            .is_some_and(|native| value.op.is_none_or(|op| !native.allows(op)));
    if blocked || session.error.is_some() || denied {
        value.op = None;
        value.refusal = Some(session.error.clone().unwrap_or_else(|| {
            rusty_l10n::tr(if blocked {
                "drag-refused-modal"
            } else {
                "drag-refused-effect"
            })
            .into()
        }));
    }
    hover
}

impl FerriteWindow {
    pub(super) fn drag_hover(
        &mut self,
        (pane, spot): (PaneId, Option<DropSpot>),
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.drag.session.as_mut() else {
            return;
        };
        match spot {
            Some(spot) => session.over = Some(Over::Spot(pane, spot)),
            None if matches!(session.over, Some(Over::Spot(over, _)) if over == pane) => {
                session.over = None;
            }
            None => return,
        }
        self.hovered_pane(pane);
        self.arm_spring(window, cx);
        self.show_hover(keys_of(&window.modifiers()), Some(window), cx);
    }

    pub(super) fn place_hover(
        &mut self,
        (over, inside): (Over, bool),
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.drag.session.as_mut() else {
            return;
        };
        if inside {
            session.over = Some(over);
        } else if session.over.as_ref() == Some(&over) {
            session.over = None;
        } else {
            return;
        }
        self.arm_spring(window, cx);
        self.show_hover(keys_of(&window.modifiers()), Some(window), cx);
    }

    pub(super) fn over_target(&self, over: &Over) -> Option<Target> {
        match over {
            Over::Spot(pane, spot) => self.drop_target(*pane, spot),
            Over::Tab(pane, tab) if self.changed_tab(*pane, *tab) => {
                Some(Err(Refusal::ListChanged))
            }
            Over::Tab(pane, tab) => self.tab_location(*pane, *tab).map(Ok),
            Over::Crumb(_, location) | Over::Drive(_, location) | Over::Menu(_, location) => {
                Some(Ok(location.clone()))
            }
        }
    }

    fn tab_landing(&self, pane: PaneId, index: usize) -> Option<TabLanding> {
        let tab = self.browser.state().pane(pane)?.tabs().get(index)?;
        Some(TabLanding {
            pane,
            index,
            id: tab.id(),
            location: tab.location().clone(),
        })
    }

    fn hovered_tab(&self, over: Option<&Over>) -> Option<TabLanding> {
        let Over::Tab(pane, index) = over? else {
            return None;
        };
        self.drag
            .session
            .as_ref()?
            .shown_tab
            .as_ref()
            .filter(|shown| (shown.pane, shown.index) == (*pane, *index))
            .cloned()
            .or_else(|| self.tab_landing(*pane, *index))
    }

    pub(super) fn changed_tab(&self, pane: PaneId, index: usize) -> bool {
        let shown = self
            .drag
            .session
            .as_ref()
            .and_then(|session| session.shown_tab.as_ref())
            .filter(|shown| (shown.pane, shown.index) == (pane, index));
        shown.is_some_and(|shown| self.tab_landing(pane, index).as_ref() != Some(shown))
    }

    pub(super) fn drag_modifiers(&mut self, modifiers: &Modifiers, cx: &mut Context<Self>) {
        self.show_hover(keys_of(modifiers), None, cx);
    }

    pub(super) fn end_finished_drag(&mut self, cx: &App) {
        if self
            .drag
            .session
            .as_ref()
            .is_some_and(|session| !session.outgoing)
            && !cx.has_active_drag()
        {
            let _ = self.end_drag();
        }
    }

    pub(super) fn drag_list_changed(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.drag.session.as_mut() else {
            return;
        };
        session.shown = None;
        let keys = session.keys;
        self.show_hover(keys, None, cx);
    }

    pub(super) fn show_hover(
        &mut self,
        keys: DropKeys,
        window: Option<&Window>,
        cx: &mut Context<Self>,
    ) {
        self.end_finished_drag(cx);
        let Some(session) = &self.drag.session else {
            return;
        };
        let keys = session.native.as_ref().map_or(keys, |native| native.keys);
        let seen = (session.over.clone(), keys);
        if session.shown.as_ref() == Some(&seen) {
            return;
        }
        let target = session
            .over
            .as_ref()
            .and_then(|over| self.over_target(over));
        let named = match (&session.over, &session.shown_target) {
            (Some(Over::Spot(pane, spot)), Some((was, at, _))) if (was, at) == (pane, spot) => {
                session.shown_target.clone()
            }
            (Some(Over::Spot(pane, spot)), _) => match &target {
                Some(Ok(location)) => Some((*pane, spot.clone(), location.clone())),
                _ => None,
            },
            _ => None,
        };
        let shown_tab = self.hovered_tab(session.over.as_ref());
        let hover = target.map(|target| hover_of(&session.carried, target, keys));
        let hover = restrict_hover(hover, session, self.drops_blocked());
        if let Some(window) = window {
            self.set_drop_effect(hover.as_ref().and_then(|hover| hover.op), window);
        }
        session.state.update(cx, |state, cx| {
            if state.hover != hover {
                state.hover = hover;
                cx.notify();
            }
        });
        if let Some(session) = self.drag.session.as_mut() {
            session.keys = keys;
            session.shown = Some(seen);
            session.shown_target = named;
            session.shown_tab = shown_tab;
        }
    }
}
