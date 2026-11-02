use super::*;

#[test]
fn pinning_moves_the_tab_to_the_end_of_the_pinned_group() {
    let mut state = with_tabs(&["/a", "/b", "/c", "/d"]);
    pin(&mut state, 2);
    pin(&mut state, 3);
    assert_eq!(folders(&state), ["/c", "/d", "/a", "/b"]);
    let pane = state.pane(state.focused_pane()).unwrap();
    assert_eq!(pane.pinned_count(), 2);
    assert_eq!(pane.active().location(), &location("/d"));
}

#[test]
fn unpinning_puts_the_tab_right_after_the_pinned_group() {
    let mut state = with_tabs(&["/a", "/b", "/c", "/d"]);
    pin(&mut state, 2);
    pin(&mut state, 3);
    pin(&mut state, 3);
    assert_eq!(folders(&state), ["/c", "/d", "/b", "/a"]);
    let first = tab_at(&state, 0);
    tab_command(
        &mut state,
        TabCommand::Pin {
            tab: Some(first),
            pinned: false,
        },
    );
    assert_eq!(folders(&state), ["/d", "/b", "/c", "/a"]);
    assert_eq!(state.pane(state.focused_pane()).unwrap().pinned_count(), 2);
}

#[test]
fn new_tabs_open_after_the_pinned_group() {
    let mut state = with_tabs(&["/a", "/b"]);
    pin(&mut state, 1);
    pin(&mut state, 1);
    run(&mut state, &Command::PaneActivateTab { tab: 0 });
    run(
        &mut state,
        &Command::PaneOpenTab {
            location: Some(location("/new")),
        },
    );
    assert_eq!(folders(&state), ["/b", "/a", "/new"]);
    assert_eq!(state.active_tab().location(), &location("/new"));
}

#[test]
fn a_pinned_tab_still_navigates() {
    let mut state = with_tabs(&["/a"]);
    pin(&mut state, 0);
    run(
        &mut state,
        &Command::PaneNavigate {
            location: location("/a/sub"),
        },
    );
    assert!(state.active_tab().is_pinned());
    assert_eq!(state.active_tab().location(), &location("/a/sub"));
}

#[test]
fn the_active_tab_is_the_default_target() {
    let mut state = with_tabs(&["/a", "/b"]);
    tab_command(
        &mut state,
        TabCommand::Pin {
            tab: None,
            pinned: true,
        },
    );
    assert_eq!(folders(&state), ["/b", "/a"]);
    assert!(state.active_tab().is_pinned());
}

#[test]
fn rename_sets_a_trimmed_label_and_empty_resets_it() {
    let mut state = with_tabs(&["/a"]);
    let tab = tab_at(&state, 0);
    let rename = |label: &str| TabCommand::Rename {
        tab: Some(tab),
        label: label.into(),
    };
    tab_command(&mut state, rename("  Builds  "));
    assert_eq!(state.active_tab().label(), Some("Builds"));
    tab_command(&mut state, rename("   "));
    assert_eq!(state.active_tab().label(), None);
}

#[test]
fn rename_refuses_a_label_over_forty_characters() {
    let mut state = with_tabs(&["/a"]);
    let label = "é".repeat(41);
    let error = refused(&mut state, TabCommand::Rename { tab: None, label });
    assert_eq!(error, StateError::InvalidTabLabel);
}

#[test]
fn a_command_on_a_missing_tab_is_refused() {
    let mut state = with_tabs(&["/a"]);
    let tab = Some(TabId::new(999));
    for command in [
        TabCommand::Pin { tab, pinned: true },
        TabCommand::Duplicate { tab },
        TabCommand::Close { tab },
        TabCommand::CloseOthers { tab },
        TabCommand::MoveToNewPane { tab },
        TabCommand::MoveToNewWorkspace { tab },
    ] {
        assert_eq!(
            refused(&mut state, command),
            StateError::UnknownTab(TabId::new(999))
        );
    }
}

#[test]
fn a_command_acts_on_a_tab_in_another_pane() {
    let mut state = with_tabs(&["/a", "/b"]);
    let left = state.focused_pane();
    let tab = tab_at(&state, 0);
    let right = split(&mut state);
    tab_command(
        &mut state,
        TabCommand::Pin {
            tab: Some(tab),
            pinned: true,
        },
    );
    let pane = state.pane(left).unwrap();
    assert_eq!(pane.tabs()[0].id(), tab);
    assert!(pane.tabs()[0].is_pinned());
    assert_eq!(state.focused_pane(), right);
}

#[test]
fn duplicate_copies_folder_and_history_next_to_the_tab() {
    let mut state = with_tabs(&["/a", "/b"]);
    run(
        &mut state,
        &Command::PaneNavigate {
            location: location("/a/deep"),
        },
    );
    let source = state.active_tab().clone();
    tab_command(&mut state, TabCommand::Duplicate { tab: None });
    assert_eq!(folders(&state), ["/a", "/a/deep", "/a/deep"]);
    let copy = state.active_tab();
    assert_ne!(copy.id(), source.id());
    assert_eq!(copy.history(), source.history());
    assert_eq!(tab_at(&state, 2), copy.id());
}

#[test]
fn duplicating_a_pinned_named_results_tab_keeps_pin_and_label_in_the_pinned_group() {
    let mut state = with_tabs(&["/a", "/b"]);
    let pane = state.focused_pane();
    let results = state.open_results_tab(pane, location("/b")).unwrap();
    tab_command(
        &mut state,
        TabCommand::Pin {
            tab: Some(results),
            pinned: true,
        },
    );
    tab_command(
        &mut state,
        TabCommand::Rename {
            tab: Some(results),
            label: "Hits".into(),
        },
    );
    tab_command(&mut state, TabCommand::Duplicate { tab: Some(results) });
    let tabs = state.pane(pane).unwrap().tabs();
    assert_eq!(tabs[0].id(), results);
    let copy = &tabs[1];
    assert_ne!(copy.id(), results);
    assert_eq!(copy.location(), &location("/b"));
    assert!(copy.is_pinned());
    assert_eq!(copy.label(), Some("Hits"));
    assert_eq!(state.active_tab().id(), copy.id());
}

#[test]
fn every_tab_command_asks_to_save_the_session() {
    let mut state = with_tabs(&["/a", "/b"]);
    let effects = tab_command(&mut state, TabCommand::Duplicate { tab: None });
    assert_eq!(effects.last(), Some(&Effect::SaveSession));
    assert!(matches!(effects[0], Effect::ListFolder { .. }));
}
