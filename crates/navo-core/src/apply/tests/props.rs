use std::collections::BTreeSet;

use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

use super::{entry, find_tab, loc};
use crate::{
    AppState, Command, CursorMove, Direction, Effect, Event, History, Listing, Location, PaneId,
    TabId, apply, apply_event,
};

const POOL: [&str; 4] = ["/home", "/a", "/b", "/c"];
const UNKNOWN: u64 = u64::MAX;

#[derive(Debug, Clone)]
enum Step {
    Run(Command),
    Answer {
        pick: usize,
        listing: Listing,
    },
    Stale {
        tab: u64,
        place: usize,
        listing: Listing,
    },
    PageRows {
        pick: Option<usize>,
        rows: u64,
    },
}

fn location() -> impl Strategy<Value = Location> {
    prop::sample::select(POOL.to_vec()).prop_map(loc)
}

fn direction() -> impl Strategy<Value = Direction> {
    prop::sample::select(Direction::ALL.to_vec())
}

fn cursor_move() -> impl Strategy<Value = CursorMove> {
    prop_oneof![
        Just(CursorMove::Up),
        Just(CursorMove::Down),
        Just(CursorMove::PageUp),
        Just(CursorMove::PageDown),
        Just(CursorMove::First),
        Just(CursorMove::Last),
        (0..60_u64).prop_map(CursorMove::Row),
    ]
}

fn pattern() -> impl Strategy<Value = String> {
    "[ab.*?; ]{0,4}"
}

fn command() -> impl Strategy<Value = Command> {
    prop_oneof![
        2 => prop::option::of(location()).prop_map(|location| Command::PaneOpenTab { location }),
        2 => prop::option::of(0..4_u32).prop_map(|tab| Command::PaneCloseTab { tab }),
        3 => location().prop_map(|location| Command::PaneNavigate { location }),
        2 => Just(Command::PaneBack),
        2 => Just(Command::PaneForward),
        4 => cursor_move().prop_map(|to| Command::PaneCursorMove { to }),
        3 => Just(Command::SelectionToggle),
        3 => Just(Command::SelectionToggleDown),
        2 => (0..60_u64).prop_map(|to| Command::SelectionRange { to }),
        2 => (pattern(), any::<bool>())
            .prop_map(|(pattern, folders)| Command::SelectionAddPattern { pattern, folders }),
        2 => (pattern(), any::<bool>())
            .prop_map(|(pattern, folders)| Command::SelectionRemovePattern { pattern, folders }),
        1 => Just(Command::SelectionInvert),
        1 => Just(Command::SelectionAll),
        1 => Just(Command::SelectionClear),
        2 => direction().prop_map(|direction| Command::LayoutSplit { direction }),
        2 => Just(Command::LayoutClose),
        2 => direction().prop_map(|direction| Command::LayoutFocus { direction }),
        1 => Just(Command::AppSaveSession),
    ]
}

fn listing() -> impl Strategy<Value = Listing> {
    prop::collection::vec(("[ab.]{1,4}", any::<bool>()), 0..=50).prop_map(|rows| {
        Listing::new(
            rows.iter()
                .map(|(name, is_folder)| entry(name, *is_folder))
                .collect(),
        )
    })
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        8 => command().prop_map(Step::Run),
        4 => (any::<usize>(), listing()).prop_map(|(pick, listing)| Step::Answer { pick, listing }),
        1 => (0..16_u64, 0..POOL.len(), listing())
            .prop_map(|(tab, place, listing)| Step::Stale { tab, place, listing }),
        1 => (prop::option::weighted(0.8, any::<usize>()), 0..30_u64)
            .prop_map(|(pick, rows)| Step::PageRows { pick, rows }),
    ]
}

fn all_panes(state: &AppState) -> Vec<PaneId> {
    state
        .window_ids()
        .into_iter()
        .filter_map(|id| state.window(id))
        .flat_map(|w| w.layout().panes())
        .collect()
}

fn event(state: &AppState, pending: &[(TabId, Location, u64)], step: &Step) -> Option<Event> {
    match step {
        Step::Run(_) => None,
        Step::Answer { pick, listing } => {
            let active = state.active_tab();
            let (tab, location, request) = pending
                .get(pick % pending.len().max(1))
                .cloned()
                .unwrap_or_else(|| (active.id(), active.location().clone(), 0));
            Some(Event::ListingLoaded {
                tab,
                location,
                request,
                listing: listing.clone(),
            })
        }
        Step::Stale {
            tab,
            place,
            listing,
        } => Some(Event::ListingLoaded {
            tab: TabId::new(*tab),
            location: loc(POOL[*place]),
            request: *tab,
            listing: listing.clone(),
        }),
        Step::PageRows { pick, rows } => {
            let panes = all_panes(state);
            let pane = pick.map_or(PaneId::new(UNKNOWN), |i| panes[i % panes.len()]);
            Some(Event::PageRowsChanged { pane, rows: *rows })
        }
    }
}

fn check_ids(state: &AppState) -> Result<(), TestCaseError> {
    let mut seen = BTreeSet::new();
    for id in state.window_ids() {
        prop_assert!(seen.insert(id.get()), "window id {} repeats", id.get());
        let window = state
            .window(id)
            .ok_or_else(|| TestCaseError::fail("missing window"))?;
        let panes: Vec<PaneId> = window.panes().iter().map(|p| p.id()).collect();
        prop_assert_eq!(window.layout().panes(), panes);
        for pane in window.panes() {
            prop_assert!(
                seen.insert(pane.id().get()),
                "pane id {} repeats",
                pane.id().get()
            );
            for tab in pane.tabs() {
                prop_assert!(
                    seen.insert(tab.id().get()),
                    "tab id {} repeats",
                    tab.id().get()
                );
            }
        }
    }
    Ok(())
}

fn check_focus(state: &AppState) -> Result<(), TestCaseError> {
    prop_assert!(state.window_ids().contains(&state.focused_window()));
    let window = state
        .window(state.focused_window())
        .ok_or_else(|| TestCaseError::fail("missing focused window"))?;
    prop_assert!(window.pane(state.focused_pane()).is_some());
    Ok(())
}

fn check_tabs(state: &AppState) -> Result<(), TestCaseError> {
    for pane_id in all_panes(state) {
        let pane = state
            .pane(pane_id)
            .ok_or_else(|| TestCaseError::fail("missing pane"))?;
        prop_assert!(!pane.tabs().is_empty());
        prop_assert!(pane.active_index() < pane.tabs().len());
        for tab in pane.tabs() {
            prop_assert!(tab.history().back().len() <= History::LIMIT);
            prop_assert!(tab.history().forward().len() <= History::LIMIT);
            let rows = tab.listing().map_or(0, Listing::len);
            prop_assert_eq!(tab.selection().len(), rows);
            if tab.listing().is_some() {
                let last = rows.saturating_sub(1);
                prop_assert!(tab.view().cursor <= last, "{:?}", tab.view());
                prop_assert!(tab.view().scroll <= last, "{:?}", tab.view());
            }
        }
    }
    Ok(())
}

fn check(state: &AppState) -> Result<(), TestCaseError> {
    check_ids(state)?;
    check_focus(state)?;
    check_tabs(state)
}

fn check_effects(state: &AppState, effects: &[Effect]) -> Result<(), TestCaseError> {
    for effect in effects {
        if let Effect::ListFolder { tab, location, .. } = effect {
            let found = find_tab(state, *tab)
                .ok_or_else(|| TestCaseError::fail(format!("{effect:?} names no tab")))?;
            prop_assert_eq!(found.location(), location);
        }
    }
    Ok(())
}

fn list_folders(effects: &[Effect]) -> impl Iterator<Item = (TabId, Location, u64)> + '_ {
    effects.iter().filter_map(|effect| match effect {
        Effect::ListFolder {
            tab,
            location,
            request,
        } => Some((*tab, location.clone(), *request)),
        _ => None,
    })
}

proptest! {
    #[test]
    fn random_commands_and_events_keep_the_state_valid(
        steps in prop::collection::vec(step(), 0..=80)
    ) {
        let mut state = AppState::new(loc("/home"));
        let mut pending = Vec::new();
        for step in &steps {
            match (step, event(&state, &pending, step)) {
                (Step::Run(command), _) => {
                    let before = state.clone();
                    match apply(&mut state, command) {
                        Ok(effects) => {
                            check_effects(&state, &effects)?;
                            pending.extend(list_folders(&effects));
                        }
                        Err(error) => prop_assert_eq!(
                            &state, &before, "{:?} failed with {:?} and changed the state", command, error
                        ),
                    }
                }
                (_, Some(event)) => {
                    let _ = apply_event(&mut state, event);
                }
                (_, None) => {}
            }
            check(&state)?;
        }
    }
}
