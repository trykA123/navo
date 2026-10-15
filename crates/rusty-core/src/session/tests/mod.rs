use super::*;
use crate::{
    Command, CursorMove, Direction, Entry, Event, History, Layout, Listing, Location, PaneId,
    TabId, ViewState, WindowId, apply, apply_event,
};

mod invalid;
mod props;
mod round_trip;
mod rows;
mod to_session;

fn loc(text: &str) -> Location {
    Location::new(text).unwrap()
}

fn home() -> AppState {
    AppState::new(loc("/home"))
}

fn run(state: &mut AppState, command: Command) {
    apply(state, &command).unwrap();
}

fn navigate(state: &mut AppState, location: &str) {
    run(
        state,
        Command::PaneNavigate {
            location: loc(location),
        },
    );
}

fn split(state: &mut AppState, direction: Direction) {
    run(state, Command::LayoutSplit { direction });
}

fn name(row: usize) -> String {
    format!("f{row}")
}

fn files(len: usize) -> Listing {
    Listing::new(
        (0..len)
            .map(|row| Entry {
                name: name(row),
                is_folder: row % 3 == 0,
            })
            .collect(),
    )
}

fn entries(rows: std::ops::Range<usize>) -> Vec<EntryState> {
    rows.map(|row| EntryState {
        name: name(row),
        is_folder: row % 3 == 0,
    })
    .collect()
}

fn load_tab(state: &mut AppState, tab: TabId, location: &str, listing: Listing) {
    if let Some(found) = state.tab_mut(tab) {
        found.expect_listing(77);
    }
    let event = Event::ListingLoaded {
        tab,
        location: loc(location),
        request: 77,
        listing,
    };
    apply_event(state, event);
}

fn load(state: &mut AppState, listing: Listing) {
    let tab = state.active_tab();
    let (id, location) = (tab.id(), tab.location().as_str().to_owned());
    load_tab(state, id, &location, listing);
}

fn tab_state(id: u64, location: &str) -> TabState {
    TabState {
        id,
        location: location.to_owned(),
        back: Vec::new(),
        forward: Vec::new(),
        cursor: 0,
        scroll: 0,
        rows: None,
    }
}

fn pane_state(id: u64, tabs: Vec<TabState>) -> PaneState {
    PaneState {
        id,
        active: 0,
        page_rows: 20,
        tabs,
    }
}

fn split_state(side_by_side: bool, first: LayoutState, second: LayoutState) -> LayoutState {
    LayoutState::Split {
        side_by_side,
        ratio: 500,
        first: Box::new(first),
        second: Box::new(second),
    }
}

// Window 1 holds panes 2 and 4 side by side; pane 4 (focused) has tabs 5 and 6.
fn valid() -> SessionState {
    SessionState {
        windows: vec![WindowState {
            id: 1,
            layout: split_state(true, LayoutState::Pane(2), LayoutState::Pane(4)),
            focused_pane: 4,
            panes: vec![
                pane_state(2, vec![tab_state(3, "/home")]),
                pane_state(4, vec![tab_state(5, "/a"), tab_state(6, "/b")]),
            ],
        }],
        focused_window: 1,
        next_id: 7,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct TabShape {
    id: TabId,
    location: Location,
    view: ViewState,
    history: History,
}

#[derive(Debug, PartialEq, Eq)]
struct PaneShape {
    id: PaneId,
    active: usize,
    page_rows: u64,
    tabs: Vec<TabShape>,
}

#[derive(Debug, PartialEq, Eq)]
struct WindowShape {
    id: WindowId,
    layout: Layout,
    panes: Vec<PaneShape>,
}

#[derive(Debug, PartialEq, Eq)]
struct Shape {
    focused_window: WindowId,
    focused_pane: PaneId,
    windows: Vec<WindowShape>,
}

// Everything the session keeps: the state without listings, selections and remembered views.
fn shape(state: &AppState) -> Shape {
    let windows = state
        .window_ids()
        .into_iter()
        .filter_map(|id| state.window(id))
        .map(|window| WindowShape {
            id: window.id(),
            layout: window.layout().clone(),
            panes: window
                .panes()
                .into_iter()
                .map(|pane| PaneShape {
                    id: pane.id(),
                    active: pane.active_index(),
                    page_rows: pane.page_rows(),
                    tabs: pane
                        .tabs()
                        .iter()
                        .map(|tab| TabShape {
                            id: tab.id(),
                            location: tab.location().clone(),
                            view: tab.view(),
                            history: tab.history().clone(),
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect();
    Shape {
        focused_window: state.focused_window(),
        focused_pane: state.focused_pane(),
        windows,
    }
}

fn all_tabs(state: &AppState) -> Vec<&crate::Tab> {
    state
        .window_ids()
        .into_iter()
        .filter_map(|id| state.window(id))
        .flat_map(|w| w.panes())
        .flat_map(|p| p.tabs())
        .collect()
}

fn busy() -> AppState {
    let mut state = home();
    navigate(&mut state, "/a");
    load(&mut state, files(40));
    run(
        &mut state,
        Command::PaneCursorMove {
            to: CursorMove::Row(30),
        },
    );
    navigate(&mut state, "/b");
    load(&mut state, files(10));
    run(&mut state, Command::PaneBack);
    load(&mut state, files(40));
    state.active_tab_mut().set_scroll(12);
    run(&mut state, Command::SelectionToggle);
    split(&mut state, Direction::Right);
    navigate(&mut state, "/c");
    run(
        &mut state,
        Command::PaneOpenTab {
            location: Some(loc("/d")),
        },
    );
    load(&mut state, files(5));
    let pane = state.focused_pane();
    apply_event(&mut state, Event::PageRowsChanged { pane, rows: 33 });
    split(&mut state, Direction::Down);
    run(
        &mut state,
        Command::LayoutFocus {
            direction: Direction::Left,
        },
    );
    let first = state.focused_window();
    state.open_window(loc("/w"));
    navigate(&mut state, "/w/x");
    split(&mut state, Direction::Left);
    state.focus_window(first).unwrap();
    state
}
