use super::{at_work, fail, files, home, load, loc, page_rows, run};
use crate::{AppState, Command, CommandError, CursorMove, Effect, StateError, WindowId};

fn open_window(state: &mut AppState, location: Option<&str>) -> Vec<Effect> {
    run(
        state,
        &Command::WindowOpen {
            location: location.map(loc),
        },
    )
}

#[test]
fn window_open_focuses_a_new_window() {
    let mut state = home();
    let first = state.focused_window();
    open_window(&mut state, Some("/work"));
    assert_ne!(state.focused_window(), first);
    assert_eq!(state.window_ids().len(), 2);
}

#[test]
fn window_open_lists_its_location() {
    let mut state = home();
    let effects = open_window(&mut state, Some("/work"));
    assert!(matches!(
        effects.as_slice(),
        [Effect::ListFolder { location, .. }] if location.as_str() == "/work"
    ));
}

#[test]
fn window_open_without_location_uses_the_active_tab_location() {
    let mut state = home();
    open_window(&mut state, None);
    assert_eq!(state.active_tab().location().as_str(), "/home");
}

#[test]
fn window_focus_returns_to_an_earlier_window() {
    let mut state = home();
    let first = state.focused_window();
    open_window(&mut state, Some("/work"));
    run(
        &mut state,
        &Command::WindowFocus {
            window: first.get(),
        },
    );
    assert_eq!(state.focused_window(), first);
}

#[test]
fn window_focus_of_an_unknown_window_fails() {
    let mut state = home();
    assert_eq!(
        fail(&mut state, &Command::WindowFocus { window: 999 }),
        CommandError::State(StateError::UnknownWindow(WindowId::new(999)))
    );
}

#[test]
fn window_close_without_id_closes_the_focused_window() {
    let mut state = home();
    let first = state.focused_window();
    open_window(&mut state, Some("/work"));
    run(&mut state, &Command::WindowClose { window: None });
    assert_eq!(state.window_ids(), vec![first]);
}

#[test]
fn window_close_of_another_window_keeps_the_focus() {
    let mut state = home();
    let first = state.focused_window();
    open_window(&mut state, Some("/work"));
    let second = state.focused_window();
    run(
        &mut state,
        &Command::WindowClose {
            window: Some(first.get()),
        },
    );
    assert_eq!(state.window_ids(), vec![second]);
}

#[test]
fn window_close_refuses_the_last_window() {
    let mut state = home();
    assert_eq!(
        fail(&mut state, &Command::WindowClose { window: None }),
        CommandError::State(StateError::LastWindow)
    );
}

#[test]
fn pane_activate_tab_switches_the_active_tab() {
    let mut state = home();
    run(
        &mut state,
        &Command::PaneOpenTab {
            location: Some(loc("/work")),
        },
    );
    run(&mut state, &Command::PaneActivateTab { tab: 0 });
    assert_eq!(state.active_tab().location().as_str(), "/home");
}

#[test]
fn pane_activate_tab_lists_an_unloaded_tab() {
    let mut state = home();
    run(
        &mut state,
        &Command::PaneOpenTab {
            location: Some(loc("/work")),
        },
    );
    let effects = run(&mut state, &Command::PaneActivateTab { tab: 0 });
    assert!(matches!(
        effects.as_slice(),
        [Effect::ListFolder { location, .. }] if location.as_str() == "/home"
    ));
}

#[test]
fn pane_activate_tab_does_not_relist_a_loaded_tab() {
    let mut state = at_work(files(3));
    run(
        &mut state,
        &Command::PaneOpenTab {
            location: Some(loc("/home")),
        },
    );
    assert_eq!(
        run(&mut state, &Command::PaneActivateTab { tab: 0 }),
        vec![]
    );
}

#[test]
fn pane_activate_tab_out_of_range_fails() {
    let mut state = home();
    assert_eq!(
        fail(&mut state, &Command::PaneActivateTab { tab: 5 }),
        CommandError::State(StateError::NoSuchTab(5))
    );
}

#[test]
fn pane_scroll_sets_the_first_visible_row() {
    let mut state = at_work(files(100));
    run(&mut state, &Command::PaneScroll { to: 40 });
    assert_eq!(state.active_tab().view().scroll, 40);
}

#[test]
fn pane_scroll_does_not_move_the_cursor() {
    let mut state = at_work(files(100));
    run(&mut state, &Command::PaneScroll { to: 40 });
    assert_eq!(state.active_tab().view().cursor, 0);
}

#[test]
fn pane_scroll_clamps_to_the_last_row() {
    let mut state = at_work(files(10));
    run(&mut state, &Command::PaneScroll { to: 400 });
    assert_eq!(state.active_tab().view().scroll, 9);
}

fn scrolled_to(state: &AppState) -> u64 {
    state.active_tab().view().scroll
}

#[test]
fn moving_the_cursor_below_the_page_scrolls_down() {
    let mut state = at_work(files(100));
    page_rows(&mut state, 10);
    run(
        &mut state,
        &Command::PaneCursorMove {
            to: CursorMove::Row(25),
        },
    );
    assert_eq!(scrolled_to(&state), 16);
}

#[test]
fn moving_the_cursor_above_the_page_scrolls_up() {
    let mut state = at_work(files(100));
    page_rows(&mut state, 10);
    run(&mut state, &Command::PaneScroll { to: 50 });
    run(
        &mut state,
        &Command::PaneCursorMove {
            to: CursorMove::Row(20),
        },
    );
    assert_eq!(scrolled_to(&state), 20);
}

#[test]
fn moving_the_cursor_inside_the_page_keeps_the_scroll() {
    let mut state = at_work(files(100));
    page_rows(&mut state, 10);
    run(&mut state, &Command::PaneScroll { to: 30 });
    run(
        &mut state,
        &Command::PaneCursorMove {
            to: CursorMove::Row(35),
        },
    );
    assert_eq!(scrolled_to(&state), 30);
}

#[test]
fn last_scrolls_the_last_row_into_view() {
    let mut state = at_work(files(100));
    page_rows(&mut state, 10);
    run(
        &mut state,
        &Command::PaneCursorMove {
            to: CursorMove::Last,
        },
    );
    assert_eq!(scrolled_to(&state), 90);
}

#[test]
fn the_default_page_keeps_a_row_near_the_top_unscrolled() {
    let mut state = at_work(files(100));
    run(
        &mut state,
        &Command::PaneCursorMove {
            to: CursorMove::Row(7),
        },
    );
    assert_eq!(scrolled_to(&state), 0);
}

#[test]
fn loading_keeps_working_after_window_commands() {
    let mut state = home();
    open_window(&mut state, Some("/work"));
    load(&mut state, files(2));
    assert_eq!(state.active_tab().listing().map(|l| l.len()), Some(2));
}

#[test]
fn holding_insert_scrolls_the_cursor_into_view() {
    let mut state = at_work(files(100));
    page_rows(&mut state, 10);
    for _ in 0..15 {
        run(&mut state, &Command::SelectionToggleDown);
    }
    assert_eq!(scrolled_to(&state), 6);
}

#[test]
fn a_range_selection_scrolls_its_end_into_view() {
    let mut state = at_work(files(100));
    page_rows(&mut state, 10);
    run(&mut state, &Command::SelectionRange { to: 40 });
    assert_eq!(scrolled_to(&state), 31);
}

#[test]
fn shrinking_the_page_keeps_the_cursor_visible() {
    let mut state = at_work(files(100));
    page_rows(&mut state, 30);
    run(
        &mut state,
        &Command::PaneCursorMove {
            to: CursorMove::Row(25),
        },
    );
    page_rows(&mut state, 10);
    assert_eq!(scrolled_to(&state), 16);
}
