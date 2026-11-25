use super::*;

fn restored(state: &AppState) -> AppState {
    AppState::from_session(&state.to_session(100)).unwrap()
}

#[test]
fn valid_session_is_accepted() {
    assert!(AppState::from_session(&valid()).is_ok());
}

#[test]
fn initial_state_round_trips_exactly() {
    let state = home();
    assert_eq!(restored(&state), state);
}

#[test]
fn busy_state_round_trips_without_listings_and_selections() {
    let state = busy();
    assert_eq!(shape(&restored(&state)), shape(&state));
}

#[test]
fn restored_tabs_have_no_listing() {
    let state = restored(&busy());
    let loaded: Vec<TabId> = all_tabs(&state)
        .into_iter()
        .filter(|t| t.listing().is_some())
        .map(|t| t.id())
        .collect();
    assert_eq!(loaded, Vec::new());
}

#[test]
fn restored_tabs_have_an_empty_selection() {
    let state = restored(&busy());
    let selected: Vec<TabId> = all_tabs(&state)
        .into_iter()
        .filter(|t| *t.selection() != crate::Selection::new(0))
        .map(|t| t.id())
        .collect();
    assert_eq!(selected, Vec::new());
}

#[test]
fn restored_view_is_kept_without_a_listing_to_clamp_it() {
    let mut session = valid();
    session.windows[0].panes[1].tabs[0].cursor = 500;
    session.windows[0].panes[1].tabs[0].scroll = 480;
    let state = AppState::from_session(&session).unwrap();
    let view = state.active_tab().view();
    assert_eq!(
        view,
        ViewState {
            cursor: 500,
            scroll: 480
        }
    );
}

#[test]
fn restored_layout_keeps_the_ratio() {
    let mut session = valid();
    session.windows[0].layout = LayoutState::Split {
        side_by_side: true,
        ratio: 350,
        first: Box::new(LayoutState::Pane(2)),
        second: Box::new(LayoutState::Pane(4)),
    };
    let state = AppState::from_session(&session).unwrap();
    let rects = state.window(WindowId::new(1)).unwrap().layout().rects();
    assert!((rects[0].1.width - 0.35).abs() < 1e-9, "{rects:?}");
}

#[test]
fn restored_state_focuses_the_saved_window_and_pane() {
    let state = AppState::from_session(&valid()).unwrap();
    assert_eq!(
        (state.focused_window(), state.focused_pane()),
        (WindowId::new(1), PaneId::new(4))
    );
}

#[test]
fn restored_rows_are_ignored_by_the_state() {
    let mut with_rows = valid();
    with_rows.windows[0].panes[0].tabs[0].rows = Some(RowsState {
        first: 0,
        total: 2,
        entries: entries(0..2),
    });
    let a = AppState::from_session(&with_rows).unwrap();
    let b = AppState::from_session(&valid()).unwrap();
    assert_eq!(a, b);
}

#[test]
fn new_ids_continue_from_next_id() {
    let mut state = AppState::from_session(&valid()).unwrap();
    split(&mut state, Direction::Right);
    assert_eq!(state.focused_pane(), PaneId::new(7));
}

fn saved(text: &str, base: &str) -> SavedFind {
    SavedFind {
        text: text.to_owned(),
        base: loc(base),
    }
}

#[test]
fn find_memory_round_trips_with_placement_pin_and_queries() {
    use crate::placement::{Monitor, MonitorLocalLogicalRect, MonitorSetup, PhysicalRect};
    let bounds = PhysicalRect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    };
    let setup = MonitorSetup::new(vec![Monitor {
        bounds,
        work_area: bounds,
        scale: 1.0,
        primary: true,
    }])
    .unwrap();
    let mut state = home();
    let memory = state.find_memory_mut();
    memory.pinned = true;
    memory.set_queries(vec![saved("*.rs", "/home"), saved("todo", "/work")]);
    memory.remember(
        &setup,
        crate::placement::Placement {
            setup: setup.fingerprint(),
            bounds: MonitorLocalLogicalRect {
                x: 5.0,
                y: 6.0,
                width: 700.0,
                height: 500.0,
            },
            monitor: bounds,
            maximized: false,
        },
    );
    let session = state.to_session(100);
    assert_eq!(session.windows[0].find_window.queries.len(), 2);
    assert_eq!(restored(&state).find_memory(), state.find_memory());
}

#[test]
fn a_saved_find_whose_base_fails_is_dropped_on_restore() {
    let mut session = home().to_session(100);
    session.windows[0].find_window.queries = vec![
        SavedFindState {
            text: "lost".into(),
            base: String::new(),
        },
        SavedFindState {
            text: "kept".into(),
            base: "/home".into(),
        },
    ];
    let state = AppState::from_session(&session).unwrap();
    assert_eq!(state.find_memory().queries(), [saved("kept", "/home")]);
}
