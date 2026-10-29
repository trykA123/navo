use super::*;
use crate::session::legacy::{PaneV4, SessionV4, SnapshotV4, TabV4, WindowV4, WorkspaceV4};
use rusty_core::{AppState, Command, Direction, SnapshotState, TabCommand, WorkspaceAction};

fn tabs_in_two_workspaces() -> SessionState {
    let mut state = AppState::from_session(&first()).unwrap();
    let run = |state: &mut AppState, command: Command| {
        rusty_core::apply(state, &command).unwrap();
    };
    run(
        &mut state,
        Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    rusty_core::apply_workspace(&mut state, &WorkspaceAction::New { name: None }).unwrap();
    run(
        &mut state,
        Command::PaneOpenTab {
            location: Some(rusty_core::Location::new("/work").unwrap()),
        },
    );
    state.to_session(0)
}

fn v4_tab(tab: &TabState) -> TabV4 {
    TabV4 {
        id: tab.id,
        location: tab.location.clone(),
        back: tab.back.clone(),
        forward: tab.forward.clone(),
        cursor: tab.cursor,
        scroll: tab.scroll,
        rows: tab.rows.clone(),
    }
}

pub(super) fn v4_panes(panes: &[PaneState]) -> Vec<PaneV4> {
    panes
        .iter()
        .map(|pane| PaneV4 {
            id: pane.id,
            active: pane.active,
            page_rows: pane.page_rows,
            tabs: pane.tabs.iter().map(v4_tab).collect(),
        })
        .collect()
}

fn v4_snapshot(snapshot: &SnapshotState) -> SnapshotV4 {
    SnapshotV4 {
        layout: snapshot.layout.clone(),
        focused_pane: snapshot.focused_pane,
        panes: v4_panes(&snapshot.panes),
        last_marks: snapshot.last_marks.clone(),
        target: snapshot.target,
    }
}

fn v4(session: &SessionState) -> SessionV4 {
    SessionV4 {
        windows: session
            .windows
            .iter()
            .map(|window| WindowV4 {
                id: window.id,
                layout: window.layout.clone(),
                focused_pane: window.focused_pane,
                panes: v4_panes(&window.panes),
                workspaces: window
                    .workspaces
                    .iter()
                    .map(|workspace| WorkspaceV4 {
                        name: workspace.name.clone(),
                        saved: v4_snapshot(&workspace.saved),
                        live: workspace.live.as_ref().map(v4_snapshot),
                    })
                    .collect(),
                active_workspace: window.active_workspace,
                placement: window.placement.clone(),
                target: window.target,
            })
            .collect(),
        focused_window: session.focused_window,
        next_id: session.next_id,
    }
}

#[test]
fn a_v4_file_loads_every_workspace_with_no_pins_or_labels() {
    let dir = tmp();
    let path = session_file(&dir);
    let session = tabs_in_two_workspaces();
    let bytes = framed(4, &postcard::to_stdvec(&v4(&session)).unwrap());
    fs::write(&path, &bytes).unwrap();
    let (loaded, source, errors) = load_session(&path);
    assert_eq!((loaded, source), (Some(session), SessionSource::Main));
    assert!(errors.is_empty());
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn pins_and_labels_round_trip_through_the_session_file() {
    let dir = tmp();
    let path = session_file(&dir);
    let mut state = AppState::from_session(&tabs_in_two_workspaces()).unwrap();
    for command in [
        TabCommand::Pin {
            tab: None,
            pinned: true,
        },
        TabCommand::Rename {
            tab: None,
            label: "Work \"📁\"".into(),
        },
    ] {
        rusty_core::apply(&mut state, &Command::Tab(command)).unwrap();
    }
    rusty_core::apply_workspace(&mut state, &WorkspaceAction::Switch { n: 1 }).unwrap();
    let session = state.to_session(0);
    write_session(&path, &session).unwrap();
    let loaded = read_session_file(&path).unwrap();
    assert_eq!(loaded, session);
    let kept = loaded.windows[0].workspaces[1].saved.panes.iter();
    let marked: Vec<_> = kept
        .flat_map(|pane| &pane.tabs)
        .filter(|tab| tab.pinned)
        .map(|tab| tab.label.as_deref())
        .collect();
    assert_eq!(marked, [Some("Work \"📁\"")]);
}

#[test]
fn a_v4_body_is_not_read_as_the_current_version() {
    let dir = tmp();
    let path = session_file(&dir);
    let session = tabs_in_two_workspaces();
    let v4_body = postcard::to_stdvec(&v4(&session)).unwrap();
    fs::write(&path, framed(SESSION_VERSION, &v4_body)).unwrap();
    assert!(read_session_file(&path).is_err());
}

const V4_SESSION: &[u8] = include_bytes!("../../../tests/fixtures/session/v4.session");

#[test]
fn frozen_v4_file_loads_with_no_pins_or_labels() {
    let expected = tabs_in_two_workspaces();
    assert_eq!(
        framed(4, &postcard::to_stdvec(&v4(&expected)).unwrap()),
        V4_SESSION
    );
    let dir = tmp();
    let path = session_file(&dir);
    fs::write(&path, V4_SESSION).unwrap();
    let (loaded, source, errors) = load_session(&path);
    assert_eq!(source, SessionSource::Main);
    assert!(errors.is_empty());
    let loaded = loaded.unwrap();
    assert_eq!(loaded, expected);
    let window = &loaded.windows[0];
    let snapshots = window
        .workspaces
        .iter()
        .flat_map(|workspace| std::iter::once(&workspace.saved).chain(workspace.live.as_ref()));
    let tabs = (window.panes.iter())
        .chain(snapshots.flat_map(|snapshot| &snapshot.panes))
        .flat_map(|pane| &pane.tabs);
    assert!(tabs.clone().count() > 0);
    assert!(
        tabs.into_iter()
            .all(|tab| !tab.pinned && tab.label.is_none())
    );
}
