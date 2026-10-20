use gpui_kit::{AnyWindowHandle, Modifiers, Subscription};
use rusty_core::{DropKeys, DropOp, Location, OpKind, PaneId, ProviderPath, drop_op};
use rusty_l10n::tr;

use super::FerriteWindow;
use crate::browser::{DropRequest, PasteRefusal, drop_refusal, same_volume};
use crate::drag::{DragBadge, DragState, FileDrag, Hover, Refusal};
use crate::file_list::{DropEvent, DropPayload, DropSpot, FileList};
use crate::modifier_hints::HintsMode;
use crate::pane_chrome::path_segments;
use crate::prelude::*;
use crate::status_bar::StatusNotice;

pub(super) struct DragSession {
    sources: Vec<Location>,
    source_pane: PaneId,
    pub(super) state: Entity<DragState>,
    over: Option<(PaneId, DropSpot)>,
    _escape: Subscription,
}

fn keys_of(modifiers: &Modifiers) -> DropKeys {
    DropKeys {
        ctrl: modifiers.control,
        shift: modifiers.shift,
        alt: modifiers.alt,
    }
}

fn op_for(sources: &[Location], target: &Location, keys: DropKeys) -> DropOp {
    let same = sources
        .first()
        .is_some_and(|source| same_volume(source, target));
    drop_op(same, keys)
}

fn refusal_of(refused: PasteRefusal) -> Option<Refusal> {
    match refused {
        PasteRefusal::AlreadyHere => Some(Refusal::SameFolder),
        PasteRefusal::IntoItself => Some(Refusal::IntoItself),
        PasteRefusal::Nothing => None,
    }
}

fn label_of(location: &Location) -> SharedString {
    path_segments(location)
        .pop()
        .map(|segment| segment.label)
        .unwrap_or_else(|| location.as_str().to_owned().into())
}

fn hover_of(sources: &[Location], target: &Location, keys: DropKeys) -> Hover {
    let refusal = drop_refusal(target, sources)
        .and_then(refusal_of)
        .map(Refusal::text);
    Hover {
        target_label: label_of(target),
        op: refusal.is_none().then(|| op_for(sources, target, keys)),
        refusal,
    }
}

fn info(text: impl Into<SharedString>) -> StatusNotice {
    StatusNotice {
        text: text.into(),
        is_error: false,
    }
}

fn empty_badge(cx: &mut App) -> Entity<DragBadge> {
    let state = cx.new(|_| DragState {
        count: 0,
        first_name: SharedString::default(),
        hover: None,
        keys_line: false,
    });
    cx.new(|cx| DragBadge::new(state, cx))
}

fn escape_cancels(window: &Window, cx: &mut Context<FerriteWindow>) -> Subscription {
    let view = cx.weak_entity();
    let own: AnyWindowHandle = window.window_handle();
    cx.intercept_keystrokes(move |event, window, cx| {
        let is_escape = event.keystroke.key == "escape";
        if !is_escape || window.window_handle() != own || !cx.has_active_drag() {
            return;
        }
        cx.stop_active_drag(window);
        cx.stop_propagation();
        let _ = view.update(cx, |view, cx| {
            view.drag_session = None;
            cx.notify();
        });
    })
}

impl FerriteWindow {
    pub(super) fn wire_drops(list: FileList, pane: PaneId, cx: &mut Context<Self>) -> FileList {
        let view = cx.weak_entity();
        list.pane(pane)
            .on_drag_start(move |drag, window, cx| {
                view.update(cx, |view, cx| view.start_drag(drag, window, cx))
                    .unwrap_or_else(|_| empty_badge(cx))
            })
            .on_hover(
                cx.listener(move |view, spot: &Option<DropSpot>, window, cx| {
                    view.drag_hover((pane, *spot), keys_of(&window.modifiers()), cx);
                }),
            )
            .on_drop(cx.listener(|view, event: &DropEvent, window, cx| {
                view.file_dropped(event, window, cx);
            }))
    }

    fn drag_sources(&self, drag: &FileDrag) -> (Vec<Location>, SharedString) {
        let Some(tab) = self
            .browser
            .state()
            .pane(drag.pane)
            .map(|pane| pane.active())
        else {
            return (Vec::new(), SharedString::default());
        };
        let Some(listing) = tab.listing() else {
            return (Vec::new(), SharedString::default());
        };
        let selection = tab.selection();
        let rows = match selection.is_marked(drag.row) {
            true => selection.marked(),
            false => vec![drag.row],
        };
        let entries: Vec<_> = rows
            .into_iter()
            .filter_map(|row| listing.get(row))
            .collect();
        let first = entries
            .first()
            .map(|entry| SharedString::from(entry.name.clone()))
            .unwrap_or_default();
        let sources = entries
            .into_iter()
            .filter_map(|entry| {
                let name = ProviderPath::from_text(&entry.name);
                tab.location().join(&name).ok()
            })
            .collect();
        (sources, first)
    }

    fn start_drag(
        &mut self,
        drag: &FileDrag,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<DragBadge> {
        self.focus_pane(drag.pane);
        self.focus_core_pane(window, cx);
        let (sources, first_name) = self.drag_sources(drag);
        let state = cx.new(|_| DragState {
            count: sources.len(),
            first_name,
            hover: None,
            keys_line: self.values.modifier_hints() != HintsMode::Off,
        });
        self.drag_session = Some(DragSession {
            sources,
            source_pane: drag.pane,
            state: state.clone(),
            over: None,
            _escape: escape_cancels(window, cx),
        });
        cx.notify();
        cx.new(|cx| DragBadge::new(state, cx))
    }

    fn drop_target(&self, pane: PaneId, spot: DropSpot) -> Option<Location> {
        let tab = self.browser.state().pane(pane)?.active();
        let folder = tab.location().clone();
        let DropSpot::Row(row) = spot else {
            return Some(folder);
        };
        let entry = tab.listing().and_then(|listing| listing.get(row));
        match entry {
            Some(entry) if entry.is_folder => {
                folder.join(&ProviderPath::from_text(&entry.name)).ok()
            }
            _ => Some(folder),
        }
    }

    fn drag_hover(
        &mut self,
        (pane, spot): (PaneId, Option<DropSpot>),
        keys: DropKeys,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.drag_session.as_mut() else {
            return;
        };
        match spot {
            Some(spot) => session.over = Some((pane, spot)),
            None if session.over.is_some_and(|(over, _)| over == pane) => session.over = None,
            None => return,
        }
        self.show_hover(keys, cx);
    }

    pub(super) fn drag_modifiers(&mut self, modifiers: &Modifiers, cx: &mut Context<Self>) {
        if self.drag_session.is_some() && !cx.has_active_drag() {
            self.drag_session = None;
        }
        self.show_hover(keys_of(modifiers), cx);
    }

    fn show_hover(&self, keys: DropKeys, cx: &mut Context<Self>) {
        let Some(session) = &self.drag_session else {
            return;
        };
        let hover = session
            .over
            .and_then(|(pane, spot)| self.drop_target(pane, spot))
            .map(|target| hover_of(&session.sources, &target, keys));
        session.state.update(cx, |state, cx| {
            if state.hover != hover {
                state.hover = hover;
                cx.notify();
            }
        });
    }

    fn file_dropped(&mut self, event: &DropEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(target) = self.drop_target(event.pane, event.spot) {
            self.drop_on((target, &event.payload), window, cx);
        }
    }

    pub(super) fn drop_on(
        &mut self,
        (target, payload): (Location, &DropPayload),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let DropPayload::Internal(drag) = payload else {
            return;
        };
        let Some(session) = self
            .drag_session
            .take()
            .filter(|session| session.source_pane == drag.pane)
        else {
            return;
        };
        self.run_drop(session.sources, target, keys_of(&window.modifiers()));
        self.focus_pane(session.source_pane);
        self.focus_core_pane(window, cx);
        cx.notify();
    }

    fn run_drop(&mut self, sources: Vec<Location>, target: Location, keys: DropKeys) {
        if let Some(refused) = drop_refusal(&target, &sources).and_then(refusal_of) {
            self.show_notice(info(refused.text()));
            return;
        }
        let kind = match op_for(&sources, &target, keys) {
            DropOp::Copy => OpKind::Copy,
            DropOp::Move => OpKind::Move,
            DropOp::Link => {
                self.show_notice(info(tr("drag-link-later")));
                return;
            }
        };
        let request = DropRequest {
            kind,
            sources,
            target,
            confirm: self.values.drop_confirm(),
        };
        match self.browser.drop_into(request) {
            Ok(_) => self.forget_clipboard_notice(),
            Err(refused) => {
                if let Some(refused) = refusal_of(refused) {
                    self.show_notice(info(refused.text()));
                }
            }
        }
    }
}
