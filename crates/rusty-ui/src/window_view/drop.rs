use gpui_kit::{AnyWindowHandle, Modifiers, Subscription};
use rusty_core::{DropKeys, DropOp, Location, PaneId, Tab, TabId, drop_op};

use super::FerriteWindow;
use super::drag_hover::Over;
use crate::browser::{PasteRefusal, same_volume};
use crate::drag::{DragBadge, DragState, FileDrag, Refusal};
use crate::file_list::{DropEvent, DropPayload, DropSpot, FileList, ListEdge};
use crate::modifier_hints::HintsMode;
use crate::pane_chrome::{PaneTabs, PathBar};
use crate::prelude::*;
use crate::status_bar::StatusNotice;

pub(super) struct DragSession {
    pub(super) carried: Carried,
    pub(super) source_pane: PaneId,
    pub(super) state: Entity<DragState>,
    pub(super) over: Option<Over>,
    pub(super) keys: DropKeys,
    pub(super) shown: Option<(Option<Over>, DropKeys)>,
    pub(super) shown_target: Option<(PaneId, DropSpot, Location)>,
    pub(super) shown_tab: Option<super::drag_hover::TabLanding>,
    pub(super) external: Option<gpui_kit::ExternalPaths>,
    pub(super) outgoing: bool,
    pub(super) native: Option<super::drop_external::NativeDrop>,
    pub(super) error: Option<SharedString>,
    pub(super) spring: super::spring::Spring,
    pub(super) _escape: Subscription,
}

pub(super) struct Carried {
    pub(super) sources: Vec<Location>,
    pub(super) parents: Vec<Location>,
    pub(super) folders: Vec<Location>,
    pub(super) first: SharedString,
    pub(super) tab: Option<TabId>,
}

pub(super) type Target = Result<Location, Refusal>;

pub(super) fn keys_of(modifiers: &Modifiers) -> DropKeys {
    DropKeys {
        ctrl: modifiers.control,
        shift: modifiers.shift,
        alt: modifiers.alt,
    }
}

pub(super) fn op_for(sources: &[Location], target: &Location, keys: DropKeys) -> DropOp {
    let same = !sources.is_empty() && sources.iter().all(|source| same_volume(source, target));
    drop_op(same, keys)
}

pub(super) fn refusal_of(refused: PasteRefusal) -> Option<Refusal> {
    match refused {
        PasteRefusal::AlreadyHere => Some(Refusal::SameFolder),
        PasteRefusal::IntoItself => Some(Refusal::IntoItself),
        PasteRefusal::IntoResults => Some(Refusal::Results),
        PasteRefusal::Nothing => None,
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

pub(super) fn escape_cancels(window: &Window, cx: &mut Context<FerriteWindow>) -> Subscription {
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
            view.set_drop_effect(None, window);
            if let Some(session) = view.end_drag() {
                view.focus_pane(session.source_pane);
            }
            cx.notify();
        });
    })
}

impl FerriteWindow {
    pub(super) fn wire_drops(list: FileList, pane: PaneId, cx: &mut Context<Self>) -> FileList {
        let view = cx.weak_entity();
        let outgoing = cx.weak_entity();
        list.pane(pane)
            .on_drag_start(move |drag, window, cx| {
                view.update(cx, |view, cx| view.start_drag(drag, window, cx))
                    .unwrap_or_else(|_| empty_badge(cx))
            })
            .on_drag_out(move |drag, _, cx| {
                outgoing
                    .update(cx, |view, _| view.drag_out_payload(drag))
                    .ok()
                    .flatten()
            })
            .on_hover(
                cx.listener(move |view, spot: &Option<DropSpot>, window, cx| {
                    view.drag_hover((pane, spot.clone()), window, cx);
                }),
            )
            .on_edge(
                cx.listener(move |view, edge: &Option<ListEdge>, window, cx| {
                    view.drag_edge((pane, *edge), window, cx);
                }),
            )
            .on_drop(cx.listener(|view, event: &DropEvent, window, cx| {
                view.file_dropped(event, window, cx);
            }))
    }

    fn drag_sources(&self, drag: &FileDrag) -> Option<Carried> {
        let tab = self.browser.state().pane(drag.pane)?.active();
        let listing = tab.listing()?;
        if listing.get(drag.row)?.name != drag.name.as_ref() {
            return None;
        }
        let selection = tab.selection();
        let rows = match selection.is_marked(drag.row) {
            true => selection.marked(),
            false => vec![drag.row],
        };
        let entries: Vec<_> = rows
            .into_iter()
            .filter_map(|row| listing.get(row))
            .collect();
        let join = |name: &str| tab.entry_location(name).ok();
        let sources: Vec<Location> = entries
            .iter()
            .filter_map(|entry| join(&entry.name))
            .collect();
        Some(Carried {
            parents: parents_of(tab, &sources),
            sources,
            folders: entries
                .iter()
                .filter(|entry| entry.is_folder)
                .filter_map(|entry| join(&entry.name))
                .collect(),
            first: entries
                .first()
                .map(|entry| SharedString::from(entry.name.clone()))
                .unwrap_or_default(),
            tab: Some(tab.id()),
        })
    }

    fn start_drag(
        &mut self,
        drag: &FileDrag,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<DragBadge> {
        self.focus_pane(drag.pane);
        self.focus_core_pane(window, cx);
        let Some(carried) = self.drag_sources(drag) else {
            self.drag.session = None;
            self.show_notice(info(Refusal::ListChanged.text()));
            window.defer(cx, |window, cx| {
                cx.stop_active_drag(window);
            });
            cx.notify();
            return empty_badge(cx);
        };
        let state = cx.new(|_| DragState {
            count: carried.sources.len(),
            first_name: carried.first.clone(),
            hover: None,
            keys_line: self.values.modifier_hints() != HintsMode::Off,
        });
        self.drag.session = Some(DragSession {
            carried,
            source_pane: drag.pane,
            state: state.clone(),
            over: None,
            keys: keys_of(&window.modifiers()),
            shown: None,
            shown_target: None,
            shown_tab: None,
            external: None,
            outgoing: false,
            native: None,
            error: None,
            spring: super::spring::Spring::default(),
            _escape: escape_cancels(window, cx),
        });
        cx.notify();
        cx.new(|cx| DragBadge::new(state, cx))
    }

    pub(super) fn drop_target(&self, pane: PaneId, spot: &DropSpot) -> Option<Target> {
        let tab = self.browser.state().pane(pane)?.active();
        row_target(tab, spot)
    }

    pub(super) fn wire_tab_drops(tabs: PaneTabs, pane: PaneId, cx: &mut Context<Self>) -> PaneTabs {
        let (hover, drop) = (cx.weak_entity(), cx.weak_entity());
        tabs.on_hover(move |tab, inside, window, cx| {
            let _ = hover.update(cx, |view, cx| {
                view.place_hover((Over::Tab(pane, *tab), inside), window, cx);
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
            let _ = hover.update(cx, |view, cx| {
                let over = Over::Crumb(pane, crumb.clone());
                view.place_hover((over, inside), window, cx);
            });
        })
        .on_drop(move |crumb, payload, window, cx| {
            let _ = drop.update(cx, |view, cx| {
                view.drop_on((crumb.clone(), payload), window, cx);
            });
        })
    }

    fn drop_on_tab(
        &mut self,
        (pane, tab, payload): (PaneId, usize, &DropPayload),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.changed_tab(pane, tab) {
            self.refuse_drop(payload, window, cx);
        } else if let Some(target) = self.tab_location(pane, tab) {
            self.drop_on((target, payload), window, cx);
        }
    }

    pub(super) fn tab_location(&self, pane: PaneId, tab: usize) -> Option<Location> {
        let pane = self.browser.state().pane(pane)?;
        pane.tabs().get(tab).map(|tab| tab.location().clone())
    }

    fn file_dropped(&mut self, event: &DropEvent, window: &mut Window, cx: &mut Context<Self>) {
        let event = &self.landed(event.clone());
        match self.drop_target(event.pane, &event.spot) {
            Some(Ok(target)) if !self.badge_named_other(event, &target) => {
                self.drop_on((target, &event.payload), window, cx);
            }
            Some(_) => self.refuse_drop(&event.payload, window, cx),
            None => {}
        }
    }

    fn badge_named_other(&self, event: &DropEvent, target: &Location) -> bool {
        let shown = self
            .drag
            .session
            .as_ref()
            .and_then(|session| session.shown_target.as_ref());
        let Some((pane, spot, named)) = shown else {
            return false;
        };
        let same_spot = match (spot, &event.spot) {
            (DropSpot::Row(shown, _), DropSpot::Row(row, _)) => shown == row,
            (shown, spot) => shown == spot,
        };
        *pane == event.pane && same_spot && !named.same_place(target)
    }

    fn refuse_drop(&mut self, payload: &DropPayload, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.take_drag(payload) else {
            return;
        };
        self.set_drop_effect(None, window);
        self.show_notice(info(Refusal::ListChanged.text()));
        self.focus_pane(session.source_pane);
        self.focus_core_pane(window, cx);
        cx.notify();
    }

    pub(super) fn drop_on(
        &mut self,
        (target, payload): (Location, &DropPayload),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.take_drag(payload) else {
            return;
        };
        let source_pane = session.source_pane;
        let restore = !self.drops_blocked();
        self.finish_drop((session, target), window, cx);
        if restore && self.drag.menu.is_none() {
            self.focus_pane(source_pane);
            self.focus_core_pane(window, cx);
        }
        cx.notify();
    }

    fn take_drag(&mut self, payload: &DropPayload) -> Option<DragSession> {
        let matches = self
            .drag
            .session
            .as_ref()
            .is_some_and(|session| match payload {
                DropPayload::Internal(drag) => {
                    session.external.is_none() && session.source_pane == drag.pane
                }
                DropPayload::External(paths) => session.external.as_ref() == Some(paths),
            });
        if matches { self.end_drag() } else { None }
    }
}

fn parents_of(tab: &Tab, sources: &[Location]) -> Vec<Location> {
    if tab.base_or_location() == tab.location() {
        return vec![tab.location().clone()];
    }
    let mut parents: Vec<Location> = Vec::new();
    for parent in sources.iter().filter_map(Location::parent) {
        if !parents.contains(&parent) {
            parents.push(parent);
        }
    }
    parents
}

fn row_target(tab: &Tab, spot: &DropSpot) -> Option<Target> {
    let folder = tab.location();
    let DropSpot::Row(row, name) = spot else {
        return Some(Ok(folder.clone()));
    };
    let entry = tab
        .listing()
        .and_then(|listing| listing.get(*row))
        .filter(|entry| entry.is_folder && entry.name == name.as_ref());
    match entry {
        Some(entry) => tab.entry_location(&entry.name).ok().map(Ok),
        None => Some(Err(Refusal::ListChanged)),
    }
}

#[cfg(test)]
mod tests {

    fn based_tab() -> rusty_core::Tab {
        use rusty_core::{AppState, Entry, Event, Listing, Location, apply_event, list_missing};
        let location = Location::new(r"C:\shown").unwrap();
        let mut state = AppState::new(location.clone());
        list_missing(&mut state);
        let tab = state.active_tab().id();
        let request = state.active_tab().pending_request().unwrap();
        apply_event(
            &mut state,
            Event::ListingLoaded {
                tab,
                request,
                location,
                listing: Listing::new(vec![Entry {
                    name: r"sub\a.txt".to_owned(),
                    is_folder: true,
                }]),
            },
        );
        let mut tab = state.active_tab().clone();
        tab.set_base(Some(Location::new(r"C:\base").unwrap()));
        tab
    }

    #[test]
    fn a_drop_onto_a_results_tab_is_refused_with_the_message() {
        use crate::browser::{PasteRefusal, drop_refusal, refusal_from};
        use rusty_core::Location;
        let results = Location::new("find://7").unwrap();
        let parents = [Location::new(r"C:\w").unwrap()];
        let sources = [Location::new(r"C:\w\a.c").unwrap()];
        let refused = drop_refusal(&results, &sources);
        assert_eq!(refused, Some(PasteRefusal::IntoResults));
        assert_eq!(
            refusal_from(&results, (&parents, &[])),
            Some(PasteRefusal::IntoResults)
        );
        let text = refused.and_then(super::refusal_of).map(|r| r.text());
        assert_eq!(
            text.as_deref(),
            Some("Search results are not a folder. Choose a real folder.")
        );
    }

    #[test]
    fn folder_row_drop_resolves_a_results_entry_under_its_base() {
        let spot = crate::file_list::DropSpot::Row(0, r"sub\a.txt".into());
        assert_eq!(
            super::row_target(&based_tab(), &spot),
            Some(Ok(rusty_core::Location::new(r"C:\base\sub\a.txt").unwrap()))
        );
    }
}
