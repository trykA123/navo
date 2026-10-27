use super::{fail, home, loc, navigate, run};
use crate::{AppState, Command, CommandError, Direction, Effect, LayoutError, PaneId, StateError};

fn numbered(state: &AppState) -> Vec<PaneId> {
    state
        .window(state.focused_window())
        .unwrap()
        .layout()
        .numbered()
}

fn target(state: &AppState) -> Option<PaneId> {
    state.window(state.focused_window()).unwrap().target()
}

fn preset(state: &mut AppState, index: usize) -> crate::Applied {
    state.apply_tiles(&crate::Tiles::builtin()[index].tiles)
}

fn tab_ids(state: &AppState, pane: PaneId) -> Vec<crate::TabId> {
    state
        .pane(pane)
        .unwrap()
        .tabs()
        .iter()
        .map(crate::Tab::id)
        .collect()
}

#[test]
fn smaller_preset_moves_every_removed_tab_in_pane_and_tab_order() {
    let mut state = home();
    preset(&mut state, 3);
    let original = numbered(&state);
    for n in 1..=4 {
        state.focus_number(n).unwrap();
        for i in 0..2 {
            let tab = state.new_tab(loc(&format!("/pane-{n}/tab-{i}")));
            state.focused_pane_mut().open_tab(tab);
        }
    }
    let active = state.active_tab().id();
    let expected: Vec<_> = original[1..]
        .iter()
        .flat_map(|&id| tab_ids(&state, id))
        .collect();
    let count = original
        .iter()
        .map(|&id| tab_ids(&state, id).len())
        .sum::<usize>();
    let applied = preset(&mut state, 0);
    assert_eq!(applied.removed, vec![original[3], original[2]]);
    assert_eq!(numbered(&state), original[..2]);
    assert_eq!(tab_ids(&state, original[1]), expected);
    assert_eq!(
        numbered(&state)
            .iter()
            .map(|&id| tab_ids(&state, id).len())
            .sum::<usize>(),
        count
    );
    assert!(state.tab(active).is_some());
    assert_eq!(state.focused_pane(), original[0]);
    assert_eq!(target(&state), Some(original[1]));
}

#[test]
fn larger_preset_keeps_panes_focus_and_folders_and_allocates_unique_tabs() {
    let mut state = home();
    navigate(&mut state, "/first");
    split(&mut state, Direction::Right);
    navigate(&mut state, "/focused");
    let original = numbered(&state);
    let active = state.active_tab().id();
    let applied = preset(&mut state, 3);
    let ids = numbered(&state);
    assert_eq!(&ids[..2], &original);
    assert_eq!(&ids[2..], &applied.added);
    assert_eq!(state.focused_pane(), original[1]);
    assert_eq!(state.active_tab().id(), active);
    assert_eq!(
        state.pane(original[0]).unwrap().active().location(),
        &loc("/first")
    );
    for id in applied.added {
        let pane = state.pane(id).unwrap();
        assert_eq!(pane.tabs().len(), 1);
        assert_eq!(pane.active().location(), &loc("/focused"));
    }
    let tabs: Vec<_> = ids.iter().flat_map(|&id| tab_ids(&state, id)).collect();
    assert_eq!(
        tabs.iter().collect::<std::collections::BTreeSet<_>>().len(),
        tabs.len()
    );
    let before = state.clone();
    let applied = preset(&mut state, 3);
    assert!(applied.added.is_empty() && applied.removed.is_empty());
    assert_eq!(state, before);
}

#[test]
fn target_tracks_focus_and_falls_back_when_removed() {
    let mut state = home();
    assert_eq!(target(&state), None);
    preset(&mut state, 1);
    let ids = numbered(&state);
    assert_eq!(target(&state), Some(ids[1]));
    state.focus_number(3).unwrap();
    assert_eq!(target(&state), Some(ids[0]));
    state.focus_number(2).unwrap();
    assert_eq!(target(&state), Some(ids[2]));
    state.focus_number(2).unwrap();
    assert_eq!(target(&state), Some(ids[2]));
    preset(&mut state, 0);
    assert_eq!(state.focused_pane(), ids[1]);
    assert_eq!(target(&state), Some(ids[0]));
    run(
        &mut state,
        Command::LayoutFocus {
            direction: Direction::Left,
        },
    );
    assert_eq!(target(&state), Some(ids[1]));
}

#[test]
fn closing_target_keeps_its_tabs_and_selects_lowest_numbered_other_pane() {
    let mut state = home();
    preset(&mut state, 1);
    let ids = numbered(&state);
    state.focus_number(3).unwrap();
    state.focus_number(1).unwrap();
    let mut expected = tab_ids(&state, ids[1]);
    expected.extend(tab_ids(&state, ids[2]));
    state.close_pane(ids[2]).unwrap();
    assert_eq!(target(&state), Some(ids[1]));
    assert_eq!(tab_ids(&state, ids[1]), expected);
}

#[test]
fn closing_focused_pane_keeps_all_its_tabs() {
    let mut state = home();
    split(&mut state, Direction::Right);
    let ids = numbered(&state);
    let tab = state.new_tab(loc("/extra"));
    state.focused_pane_mut().open_tab(tab);
    let active = state.active_tab().id();
    let mut expected = tab_ids(&state, ids[0]);
    expected.extend(tab_ids(&state, ids[1]));
    run(&mut state, Command::LayoutClose);
    assert_eq!(tab_ids(&state, ids[0]), expected);
    assert!(state.tab(active).is_some());
    assert_eq!(target(&state), None);
}

#[test]
fn fifth_split_refuses_without_changing_state() {
    let mut state = home();
    preset(&mut state, 3);
    let before = state.clone();
    assert_eq!(
        fail(
            &mut state,
            Command::LayoutSplit {
                direction: Direction::Right
            }
        ),
        CommandError::State(StateError::TooManyPanes)
    );
    assert_eq!(state, before);
    assert_eq!(
        StateError::TooManyPanes.to_string(),
        "Four panes is the most in this version."
    );
}

#[test]
fn layout_operations_keep_panes_and_focus_and_reject_invalid_numbers() {
    let mut state = home();
    preset(&mut state, 1);
    let ids = numbered(&state);
    let focused = state.focused_pane();
    state.swap(ids[0], ids[2]).unwrap();
    assert_eq!(numbered(&state), vec![ids[2], ids[1], ids[0]]);
    assert_eq!(state.focused_pane(), focused);
    let root = crate::SplitPath(vec![]);
    state.set_ratio(&root, 700).unwrap();
    assert_eq!(
        state
            .window(state.focused_window())
            .unwrap()
            .layout()
            .splits()[0]
            .ratio,
        700
    );
    state.even();
    assert_eq!(
        state
            .window(state.focused_window())
            .unwrap()
            .layout()
            .splits()[0]
            .ratio,
        333
    );
    let before = state.clone();
    assert!(state.focus_number(0).is_err());
    assert!(state.focus_number(4).is_err());
    assert!(
        state
            .set_ratio(&crate::SplitPath(vec![false]), 500)
            .is_err()
    );
    assert!(state.swap(ids[0], PaneId::new(999)).is_err());
    assert_eq!(state, before);
}

#[test]
fn workspace_switch_restores_layout_and_target_and_copy_remaps_target() {
    let mut state = home();
    preset(&mut state, 2);
    state.focus_number(2).unwrap();
    state.focus_number(3).unwrap();
    let before = state.window(state.focused_window()).unwrap().snapshot();
    crate::apply_workspace(&mut state, &crate::WorkspaceAction::New { name: None }).unwrap();
    let copied = numbered(&state);
    assert_eq!(target(&state), Some(copied[1]));
    preset(&mut state, 0);
    crate::apply_workspace(&mut state, &crate::WorkspaceAction::Switch { n: 1 }).unwrap();
    assert_eq!(
        state.window(state.focused_window()).unwrap().snapshot(),
        before
    );
}

fn split(state: &mut AppState, direction: Direction) -> Vec<Effect> {
    run(state, Command::LayoutSplit { direction })
}

fn panes(state: &AppState) -> Vec<PaneId> {
    state
        .window(state.focused_window())
        .unwrap()
        .layout()
        .panes()
}

#[test]
fn split_adds_a_pane_and_focuses_it() {
    let mut state = home();
    let first = state.focused_pane();
    split(&mut state, Direction::Right);
    let second = state.focused_pane();
    assert_eq!(panes(&state), vec![first, second]);
}

#[test]
fn split_opens_the_new_pane_at_the_same_location() {
    let mut state = home();
    navigate(&mut state, "/work");
    split(&mut state, Direction::Down);
    assert_eq!(state.active_tab().location(), &loc("/work"));
}

#[test]
fn split_lists_the_new_pane_tab() {
    let mut state = home();
    super::load(&mut state, super::files(1));
    navigate(&mut state, "/work");
    super::load(&mut state, super::files(1));
    let effects = split(&mut state, Direction::Left);
    assert_eq!(
        effects,
        vec![Effect::ListFolder {
            tab: state.active_tab().id(),
            location: loc("/work"),
            request: state.active_tab().pending_request().unwrap_or(0),
        }]
    );
}

#[test]
fn close_removes_the_focused_pane() {
    let mut state = home();
    let first = state.focused_pane();
    split(&mut state, Direction::Right);
    run(&mut state, Command::LayoutClose);
    assert_eq!(panes(&state), vec![first]);
}

#[test]
fn close_has_no_effects() {
    let mut state = home();
    super::load(&mut state, super::files(1));
    split(&mut state, Direction::Right);
    assert_eq!(run(&mut state, Command::LayoutClose), vec![]);
}

#[test]
fn close_refuses_the_last_pane() {
    assert_eq!(
        fail(&mut home(), Command::LayoutClose),
        CommandError::State(StateError::Layout(LayoutError::LastPane))
    );
}

#[test]
fn focus_moves_to_the_neighbour() {
    let mut state = home();
    let first = state.focused_pane();
    split(&mut state, Direction::Right);
    run(
        &mut state,
        Command::LayoutFocus {
            direction: Direction::Left,
        },
    );
    assert_eq!(state.focused_pane(), first);
}

#[test]
fn focus_has_no_effects() {
    let mut state = home();
    super::load(&mut state, super::files(1));
    split(&mut state, Direction::Right);
    assert_eq!(
        run(
            &mut state,
            Command::LayoutFocus {
                direction: Direction::Left
            }
        ),
        vec![]
    );
}

#[test]
fn focus_without_a_neighbour_fails() {
    assert_eq!(
        fail(
            &mut home(),
            Command::LayoutFocus {
                direction: Direction::Up
            }
        ),
        CommandError::State(StateError::Layout(LayoutError::NoNeighbour(Direction::Up)))
    );
}

#[test]
fn save_session_asks_to_save_the_session() {
    assert_eq!(
        run(&mut home(), Command::AppSaveSession),
        vec![Effect::SaveSession]
    );
}

#[test]
fn save_session_leaves_the_state_alone() {
    let mut state = home();
    navigate(&mut state, "/work");
    let before = state.clone();
    run(&mut state, Command::AppSaveSession);
    assert_eq!(state, before);
}

#[test]
fn preset_command_lists_new_panes_without_relisting_loaded_tabs() {
    let mut state = home();
    super::load(&mut state, super::files(1));
    let original = state.active_tab().id();
    let effects = run(
        &mut state,
        Command::LayoutPreset {
            tiles: crate::Tiles::builtin()[3].tiles.clone(),
        },
    );
    let mut expected: Vec<_> = numbered(&state)
        .into_iter()
        .map(|id| state.pane(id).unwrap().active().id())
        .filter(|&id| id != original)
        .collect();
    let mut listed: Vec<_> = effects
        .iter()
        .map(|effect| match effect {
            Effect::ListFolder { tab, .. } => *tab,
            _ => panic!("expected folder listing"),
        })
        .collect();
    listed.sort_unstable();
    expected.sort_unstable();
    assert_eq!(listed, expected);
}

#[test]
fn swap_command_uses_reading_order_numbers() {
    let mut state = home();
    preset(&mut state, 3);
    let ids = numbered(&state);
    let focused = state.focused_pane();
    assert!(run(&mut state, Command::LayoutSwap { a: 1, b: 4 }).is_empty());
    assert_eq!(numbered(&state), vec![ids[3], ids[1], ids[2], ids[0]]);
    assert_eq!(state.focused_pane(), focused);
}

#[test]
fn swap_toward_command_moves_focused_pane_without_changing_focus() {
    let mut state = home();
    preset(&mut state, 0);
    let ids = numbered(&state);
    assert!(
        run(
            &mut state,
            Command::LayoutSwapToward {
                direction: Direction::Right
            }
        )
        .is_empty()
    );
    assert_eq!(numbered(&state), vec![ids[1], ids[0]]);
    assert_eq!(state.focused_pane(), ids[0]);
}

#[test]
fn resize_command_moves_split_by_fifty_permille() {
    let mut state = home();
    preset(&mut state, 0);
    assert!(
        run(
            &mut state,
            Command::LayoutResize {
                direction: Direction::Right
            }
        )
        .is_empty()
    );
    assert_eq!(
        state
            .window(state.focused_window())
            .unwrap()
            .layout()
            .splits()[0]
            .ratio,
        550
    );
}

#[test]
fn focus_pane_command_lists_only_when_moving_to_an_unlisted_tab() {
    let mut state = home();
    preset(&mut state, 0);
    let ids = numbered(&state);
    let effects = run(&mut state, Command::LayoutFocusPane { n: 2 });
    assert_eq!(state.focused_pane(), ids[1]);
    assert!(
        matches!(&effects[..], [Effect::ListFolder { tab, .. }] if *tab == state.active_tab().id())
    );
    assert!(run(&mut state, Command::LayoutFocusPane { n: 2 }).is_empty());
    super::load(&mut state, super::files(1));
    run(&mut state, Command::LayoutFocusPane { n: 1 });
    assert!(run(&mut state, Command::LayoutFocusPane { n: 2 }).is_empty());
}

#[test]
fn ratio_and_even_commands_update_splits() {
    let mut state = home();
    preset(&mut state, 0);
    assert!(
        run(
            &mut state,
            Command::LayoutRatio {
                split: crate::SplitPath(vec![]),
                ratio: 700
            }
        )
        .is_empty()
    );
    assert_eq!(
        state
            .window(state.focused_window())
            .unwrap()
            .layout()
            .splits()[0]
            .ratio,
        700
    );
    assert!(run(&mut state, Command::LayoutEven).is_empty());
    assert_eq!(
        state
            .window(state.focused_window())
            .unwrap()
            .layout()
            .splits()[0]
            .ratio,
        500
    );
}
