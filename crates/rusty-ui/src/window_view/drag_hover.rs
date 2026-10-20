use gpui_kit::Modifiers;
use rusty_core::{DropKeys, Location, PaneId};

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

impl FerriteWindow {
    pub(super) fn drag_hover(
        &mut self,
        (pane, spot): (PaneId, Option<DropSpot>),
        keys: DropKeys,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.drag_session.as_mut() else {
            return;
        };
        match spot {
            Some(spot) => session.over = Some(Over::Spot(pane, spot)),
            None if matches!(session.over, Some(Over::Spot(over, _)) if over == pane) => {
                session.over = None;
            }
            None => return,
        }
        self.show_hover(keys, cx);
    }

    pub(super) fn place_hover(
        &mut self,
        (over, inside): (Over, bool),
        keys: DropKeys,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.drag_session.as_mut() else {
            return;
        };
        if inside {
            session.over = Some(over);
        } else if session.over.as_ref() == Some(&over) {
            session.over = None;
        } else {
            return;
        }
        self.show_hover(keys, cx);
    }

    pub(super) fn over_target(&self, over: &Over) -> Option<Target> {
        match over {
            Over::Spot(pane, spot) => self.drop_target(*pane, spot),
            Over::Tab(pane, tab) => self.tab_location(*pane, *tab).map(Ok),
            Over::Crumb(_, location) | Over::Drive(_, location) => Some(Ok(location.clone())),
        }
    }

    pub(super) fn drag_modifiers(&mut self, modifiers: &Modifiers, cx: &mut Context<Self>) {
        self.show_hover(keys_of(modifiers), cx);
    }

    pub(super) fn end_finished_drag(&mut self, cx: &App) {
        if self.drag_session.is_some() && !cx.has_active_drag() {
            self.drag_session = None;
        }
    }

    pub(super) fn drag_list_changed(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.drag_session.as_mut() else {
            return;
        };
        session.shown = None;
        let keys = session.keys;
        self.show_hover(keys, cx);
    }

    pub(super) fn show_hover(&mut self, keys: DropKeys, cx: &mut Context<Self>) {
        self.end_finished_drag(cx);
        let Some(session) = &self.drag_session else {
            return;
        };
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
        let hover = target.map(|target| hover_of(&session.carried, target, keys));
        session.state.update(cx, |state, cx| {
            if state.hover != hover {
                state.hover = hover;
                cx.notify();
            }
        });
        if let Some(session) = self.drag_session.as_mut() {
            session.keys = keys;
            session.shown = Some(seen);
            session.shown_target = named;
        }
    }
}
