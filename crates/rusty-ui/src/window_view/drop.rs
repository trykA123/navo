use gpui_kit::{AnyWindowHandle, Modifiers, Subscription};
use rusty_core::{DropKeys, DropOp, Location, OpKind, PaneId, ProviderPath, drop_op};

use super::FerriteWindow;
use crate::browser::{DropRequest, PasteRefusal, drop_refusal, same_volume};
use crate::drag::{DragBadge, DragState, FileDrag, Hover, Refusal};
use crate::file_list::{DropEvent, DropPayload, DropSpot, FileList};
use crate::modifier_hints::HintsMode;
use crate::pane_chrome::{DrivePill, PaneTabs, PathBar, path_segments};
use crate::prelude::*;
use crate::status_bar::StatusNotice;

pub(super) struct DragSession {
    sources: Vec<Location>,
    source_pane: PaneId,
    pub(super) state: Entity<DragState>,
    over: Option<Over>,
    _escape: Subscription,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Over {
    Spot(PaneId, DropSpot),
    Tab(PaneId, usize),
    Crumb(PaneId, Location),
    Drive(PaneId, Location),
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
            Some(spot) => session.over = Some(Over::Spot(pane, spot)),
            None if matches!(session.over, Some(Over::Spot(over, _)) if over == pane) => {
                session.over = None;
            }
            None => return,
        }
        self.show_hover(keys, cx);
    }

    fn place_hover(
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

    pub(super) fn wire_tab_drops(tabs: PaneTabs, pane: PaneId, cx: &mut Context<Self>) -> PaneTabs {
        let (hover, drop) = (cx.weak_entity(), cx.weak_entity());
        tabs.on_hover(move |tab, inside, window, cx| {
            let keys = keys_of(&window.modifiers());
            let _ = hover.update(cx, |view, cx| {
                view.place_hover((Over::Tab(pane, *tab), inside), keys, cx);
            });
        })
        .on_drop(move |tab, payload, window, cx| {
            let _ = drop.update(cx, |view, cx| {
                view.drop_on_tab((pane, *tab, payload), window, cx);
            });
        })
    }

    pub(super) fn wire_crumb_drops(bar: PathBar, pane: PaneId, cx: &mut Context<Self>) -> PathBar {
        let (hover, drop) = (cx.weak_entity(), cx.weak_entity());
        bar.on_hover(move |crumb, inside, window, cx| {
            let keys = keys_of(&window.modifiers());
            let _ = hover.update(cx, |view, cx| {
                let over = Over::Crumb(pane, crumb.clone());
                view.place_hover((over, inside), keys, cx);
            });
        })
        .on_drop(move |crumb, payload, window, cx| {
            let _ = drop.update(cx, |view, cx| {
                view.drop_on((crumb.clone(), payload), window, cx);
            });
        })
    }

    pub(super) fn wire_drive_drops(
        pill: DrivePill,
        pane: PaneId,
        cx: &mut Context<Self>,
    ) -> DrivePill {
        let (hover, drop) = (cx.weak_entity(), cx.weak_entity());
        pill.on_hover(move |root, inside, window, cx| {
            let keys = keys_of(&window.modifiers());
            let _ = hover.update(cx, |view, cx| {
                let over = Over::Drive(pane, root.clone());
                view.place_hover((over, inside), keys, cx);
            });
        })
        .on_drop(move |root, payload, window, cx| {
            let _ = drop.update(cx, |view, cx| {
                view.drop_on((root.clone(), payload), window, cx);
            });
        })
    }

    fn drop_on_tab(
        &mut self,
        (pane, tab, payload): (PaneId, usize, &DropPayload),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(target) = self.tab_location(pane, tab) {
            self.drop_on((target, payload), window, cx);
        }
    }

    fn tab_location(&self, pane: PaneId, tab: usize) -> Option<Location> {
        let pane = self.browser.state().pane(pane)?;
        pane.tabs().get(tab).map(|tab| tab.location().clone())
    }

    fn over_target(&self, over: &Over) -> Option<Location> {
        match over {
            Over::Spot(pane, spot) => self.drop_target(*pane, *spot),
            Over::Tab(pane, tab) => self.tab_location(*pane, *tab),
            Over::Crumb(_, location) | Over::Drive(_, location) => Some(location.clone()),
        }
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
            .as_ref()
            .and_then(|over| self.over_target(over))
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
            DropOp::Link => OpKind::Link,
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
