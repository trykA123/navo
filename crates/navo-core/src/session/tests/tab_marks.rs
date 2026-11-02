use super::*;
use crate::TabCommand;

fn tab(state: &mut AppState, command: TabCommand) {
    run(state, &Command::Tab(command));
}

#[test]
fn pinned_tabs_and_labels_survive_a_round_trip() {
    let mut state = home();
    run(
        &mut state,
        &Command::PaneOpenTab {
            location: Some(loc("/work")),
        },
    );
    tab(
        &mut state,
        TabCommand::Pin {
            tab: None,
            pinned: true,
        },
    );
    tab(
        &mut state,
        TabCommand::Rename {
            tab: None,
            label: "Work".into(),
        },
    );
    let session = state.to_session(0);
    let saved = &session.windows[0].panes[0].tabs;
    assert_eq!(
        (saved[0].pinned, saved[0].label.as_deref()),
        (true, Some("Work"))
    );
    let restored = AppState::from_session(&session).unwrap();
    let marks = |state: &AppState| -> Vec<(TabId, bool, Option<String>)> {
        all_tabs(state)
            .iter()
            .map(|t| (t.id(), t.is_pinned(), t.label().map(str::to_owned)))
            .collect()
    };
    assert_eq!(marks(&restored), marks(&state));
}

#[test]
fn labels_and_pins_in_other_workspaces_survive_a_round_trip() {
    let mut state = home();
    tab(
        &mut state,
        TabCommand::Rename {
            tab: None,
            label: "Home".into(),
        },
    );
    run(&mut state, &Command::WorkspaceNew { name: None });
    let restored = AppState::from_session(&state.to_session(0)).unwrap();
    let window = restored.window(restored.focused_window()).unwrap();
    let first = &window.workspaces()[0].saved;
    assert_eq!(first.panes()[0].tabs()[0].label(), Some("Home"));
}

#[test]
fn a_session_without_pins_or_labels_loads_with_none() {
    let state = AppState::from_session(&valid()).unwrap();
    let tabs = all_tabs(&state);
    assert!(tabs.iter().all(|t| !t.is_pinned() && t.label().is_none()));
}

#[test]
fn a_restored_pane_puts_its_pinned_tabs_first_and_keeps_the_active_tab() {
    let mut session = valid();
    let pane = &mut session.windows[0].panes[1];
    pane.active = 0;
    pane.tabs[1].pinned = true;
    let state = AppState::from_session(&session).unwrap();
    let pane = state.pane(PaneId::new(4)).unwrap();
    let ids: Vec<u64> = pane.tabs().iter().map(|t| t.id().get()).collect();
    assert_eq!(ids, [6, 5]);
    assert_eq!(pane.active().id(), TabId::new(5));
}

#[test]
fn a_restored_label_that_is_too_long_or_blank_is_dropped() {
    let mut session = valid();
    let tabs = &mut session.windows[0].panes[1].tabs;
    tabs[0].label = Some("x".repeat(41));
    tabs[1].label = Some("   ".into());
    let state = AppState::from_session(&session).unwrap();
    assert!(all_tabs(&state).iter().all(|t| t.label().is_none()));
}
