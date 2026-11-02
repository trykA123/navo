use proptest::prelude::*;

use super::*;

const POOL: [&str; 4] = ["/home", "/a", "/b", "/c"];

#[derive(Debug, Clone)]
enum Step {
    Run(Command),
    Load { pick: usize, len: usize },
    PageRows { pick: usize, rows: u64 },
    Scroll(u64),
    OpenWindow(usize),
    FocusWindow(usize),
}

fn location() -> impl Strategy<Value = Location> {
    prop::sample::select(POOL.to_vec()).prop_map(loc)
}

fn direction() -> impl Strategy<Value = Direction> {
    prop::sample::select(Direction::ALL.to_vec())
}

fn command() -> impl Strategy<Value = Command> {
    prop_oneof![
        2 => prop::option::of(location()).prop_map(|location| Command::PaneOpenTab { location }),
        1 => prop::option::of(0..4_u32).prop_map(|tab| Command::PaneCloseTab { tab }),
        3 => location().prop_map(|location| Command::PaneNavigate { location }),
        2 => Just(Command::PaneBack),
        1 => Just(Command::PaneForward),
        2 => (0..80_u64).prop_map(|row| Command::PaneCursorMove { to: CursorMove::Row(row) }),
        1 => Just(Command::SelectionToggle),
        2 => direction().prop_map(|direction| Command::LayoutSplit { direction }),
        1 => Just(Command::LayoutClose),
        2 => direction().prop_map(|direction| Command::LayoutFocus { direction }),
    ]
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        8 => command().prop_map(Step::Run),
        4 => (any::<usize>(), 0..250_usize).prop_map(|(pick, len)| Step::Load { pick, len }),
        1 => (any::<usize>(), 0..60_u64).prop_map(|(pick, rows)| Step::PageRows { pick, rows }),
        2 => (0..250_u64).prop_map(Step::Scroll),
        1 => (0..POOL.len()).prop_map(Step::OpenWindow),
        1 => any::<usize>().prop_map(Step::FocusWindow),
    ]
}

fn pick<T: Copy>(items: &[T], index: usize) -> T {
    items[index % items.len()]
}

fn perform(state: &mut AppState, step: &Step) {
    match step {
        Step::Run(command) => {
            let _ = apply(state, command);
        }
        Step::Load { pick: index, len } => {
            let tabs: Vec<(TabId, Location)> = all_tabs(state)
                .into_iter()
                .map(|t| (t.id(), t.location().clone()))
                .collect();
            let (tab, location) = tabs[index % tabs.len()].clone();
            load_tab(state, tab, location.as_str(), files(*len));
        }
        Step::PageRows { pick: index, rows } => {
            let panes: Vec<PaneId> = state
                .window_ids()
                .into_iter()
                .filter_map(|id| state.window(id))
                .flat_map(|w| w.layout().panes())
                .collect();
            let pane = pick(&panes, *index);
            apply_event(state, Event::PageRowsChanged { pane, rows: *rows });
        }
        Step::Scroll(row) => state.active_tab_mut().set_scroll(*row),
        Step::OpenWindow(place) => {
            state.open_window(loc(POOL[*place]));
        }
        Step::FocusWindow(index) => {
            let id = pick(&state.window_ids(), *index);
            let _ = state.focus_window(id);
        }
    }
}

fn without_rows(mut session: SessionState) -> SessionState {
    for tab in session
        .windows
        .iter_mut()
        .flat_map(|w| &mut w.panes)
        .flat_map(|p| &mut p.tabs)
    {
        tab.rows = None;
    }
    session
}

proptest! {
    #[test]
    fn random_states_round_trip_through_the_session(
        steps in prop::collection::vec(step(), 0..=60),
        rows_per_pane in 0..=120_usize,
    ) {
        let mut state = home();
        for step in &steps {
            perform(&mut state, step);
        }
        let session = state.to_session(rows_per_pane);
        let restored = AppState::from_session(&session)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert_eq!(restored.to_session(rows_per_pane), without_rows(session));
    }
}
