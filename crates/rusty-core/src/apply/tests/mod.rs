use crate::{
    AppState, Command, CommandError, CursorMove, Effect, Entry, Event, Listing, Location, Tab,
    TabId, apply, apply_event,
};

mod cursor;
mod events;
mod layout;
mod pane;
mod props;
mod select;

fn loc(text: &str) -> Location {
    Location::new(text).unwrap()
}

fn entry(name: &str, is_folder: bool) -> Entry {
    Entry {
        name: name.to_owned(),
        is_folder,
    }
}

fn files(len: usize) -> Listing {
    Listing::new(
        (0..len)
            .map(|i| entry(&format!("f{i}.txt"), false))
            .collect(),
    )
}

fn mixed() -> Listing {
    Listing::new(vec![
        entry("docs", true),
        entry("a.rs", false),
        entry("b.txt", false),
        entry("src.rs", true),
        entry("c.rs", false),
    ])
}

fn home() -> AppState {
    AppState::new(loc("/home"))
}

fn run(state: &mut AppState, command: Command) -> Vec<Effect> {
    apply(state, &command).unwrap()
}

fn fail(state: &mut AppState, command: Command) -> CommandError {
    apply(state, &command).unwrap_err()
}

fn navigate(state: &mut AppState, location: &str) -> Vec<Effect> {
    run(
        state,
        Command::PaneNavigate {
            location: loc(location),
        },
    )
}

fn load(state: &mut AppState, listing: Listing) {
    if state.active_tab().pending_request().is_none() {
        run(state, Command::PaneRefresh);
    }
    let tab = state.active_tab();
    let event = Event::ListingLoaded {
        tab: tab.id(),
        location: tab.location().clone(),
        request: tab.pending_request().unwrap_or(0),
        listing,
    };
    assert_eq!(apply_event(state, event), vec![]);
}

fn at_work(listing: Listing) -> AppState {
    let mut state = home();
    navigate(&mut state, "/work");
    load(&mut state, listing);
    state
}

fn move_cursor(state: &mut AppState, to: CursorMove) {
    run(state, Command::PaneCursorMove { to });
}

fn cursor(state: &AppState) -> u64 {
    state.active_tab().view().cursor
}

fn marked(state: &AppState) -> Vec<u64> {
    state.active_tab().selection().marked()
}

fn page_rows(state: &mut AppState, rows: u64) {
    let pane = state.focused_pane();
    assert_eq!(
        apply_event(state, Event::PageRowsChanged { pane, rows }),
        vec![]
    );
}

fn find_tab(state: &AppState, id: TabId) -> Option<&Tab> {
    state
        .window_ids()
        .into_iter()
        .filter_map(|w| state.window(w))
        .flat_map(|w| w.panes())
        .flat_map(|p| p.tabs())
        .find(|t| t.id() == id)
}
