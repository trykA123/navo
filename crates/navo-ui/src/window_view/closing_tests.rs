use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gpui_kit::TestAppContext;
use navo_core::{Command, TabMove};

use super::super::tests::{close_in, fake, open_options, options_for, settle, two_panes};
use super::*;

fn open(cx: &mut TestAppContext, saves: Rc<Cell<usize>>) -> (AnyWindowHandle, Entity<NavoWindow>) {
    let fs = fake();
    let mut options = options_for(fs.clone(), two_panes(&fs));
    options.on_save = Rc::new(move |_| saves.set(saves.get() + 1));
    open_options(cx, options)
}

fn wait(cx: &mut TestAppContext, millis: u64) {
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(millis));
    cx.run_until_parked();
}

fn change(view: &Entity<NavoWindow>, cx: &mut TestAppContext) {
    view.update(cx, |view, cx| {
        view.browser
            .command(&Command::WorkspaceNew { name: None })
            .unwrap();
        cx.notify();
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_workspace_change_saves_once_after_two_seconds(cx: &mut TestAppContext) {
    let saves = Rc::new(Cell::new(0));
    let (_, view) = open(cx, saves.clone());
    change(&view, cx);
    wait(cx, 1999);
    assert_eq!(saves.get(), 0);
    wait(cx, 1);
    assert_eq!(saves.get(), 1);
    wait(cx, 4000);
    assert_eq!(saves.get(), 1);
}

#[gpui_kit::test]
fn three_workspace_changes_save_once_after_the_first_request(cx: &mut TestAppContext) {
    let saves = Rc::new(Cell::new(0));
    let (_, view) = open(cx, saves.clone());
    change(&view, cx);
    wait(cx, 700);
    change(&view, cx);
    wait(cx, 700);
    change(&view, cx);
    wait(cx, 600);
    assert_eq!(saves.get(), 1);
    wait(cx, 4000);
    assert_eq!(saves.get(), 1);
}

#[gpui_kit::test]
fn an_unchanged_window_does_not_save(cx: &mut TestAppContext) {
    let saves = Rc::new(Cell::new(0));
    let _ = open(cx, saves.clone());
    wait(cx, 6000);
    assert_eq!(saves.get(), 0);
}

#[gpui_kit::test]
fn closing_with_a_pending_save_saves_synchronously(cx: &mut TestAppContext) {
    let saves = Rc::new(Cell::new(0));
    let (window, view) = open(cx, saves.clone());
    change(&view, cx);
    assert!(close_in(cx, window, &view));
    assert_eq!(saves.get(), 1);
    wait(cx, 4000);
    assert_eq!(saves.get(), 1);
}

#[gpui_kit::test]
fn autosave_writes_the_latest_snapshot_on_the_background_executor(cx: &mut TestAppContext) {
    let (_, view) = open(cx, Rc::default());
    let snapshots = Arc::new(Mutex::new(Vec::new()));
    let sink = snapshots.clone();
    view.update(cx, |view, _| {
        view.autosave.save = Some(Arc::new(move |state| {
            sink.lock().unwrap().push(state.to_session(0));
        }));
    });
    change(&view, cx);
    wait(cx, 1000);
    change(&view, cx);
    wait(cx, 1000);
    let expected = view.read_with(cx, |view, _| view.browser.state().to_session(0));
    assert_eq!(*snapshots.lock().unwrap(), vec![expected]);
}

#[gpui_kit::test]
fn closing_finishes_the_background_write_before_the_final_save(cx: &mut TestAppContext) {
    let (window, view) = open(cx, Rc::default());
    let snapshots = Arc::new(Mutex::new(Vec::new()));
    let (background, final_save) = (snapshots.clone(), snapshots.clone());
    view.update(cx, |view, _| {
        view.autosave.save = Some(Arc::new(move |state| {
            background.lock().unwrap().push(state.to_session(0));
        }));
        view.on_save = Rc::new(move |state| {
            final_save.lock().unwrap().push(state.to_session(0));
        });
    });
    change(&view, cx);
    let first = view.read_with(cx, |view, _| view.browser.state().to_session(0));
    cx.executor()
        .scheduler_executor()
        .scheduler()
        .as_test()
        .unwrap()
        .clock()
        .advance(SAVE_AFTER);
    while view.read_with(cx, |view, _| view.autosave.in_flight.is_none()) {
        assert!(
            cx.executor().tick(),
            "autosave must reach the background write"
        );
    }
    view.update(cx, |view, cx| {
        view.browser
            .command(&Command::WorkspaceNew { name: None })
            .unwrap();
        cx.notify();
    });
    assert!(close_in(cx, window, &view));
    let last = view.read_with(cx, |view, _| view.browser.state().to_session(0));
    assert_eq!(*snapshots.lock().unwrap(), vec![first, last]);
    wait(cx, 4000);
    assert_eq!(snapshots.lock().unwrap().len(), 2);
}

#[gpui_kit::test]
fn tab_navigation_and_pane_changes_request_saves(cx: &mut TestAppContext) {
    let saves = Rc::new(Cell::new(0));
    let (_, view) = open(cx, saves.clone());
    let commands = [
        Command::PaneOpenTab { location: None },
        Command::PaneNavigate {
            location: fake().location("home/docs"),
        },
        Command::PaneBack,
        Command::PaneForward,
        Command::PaneCloseTab { tab: None },
        Command::LayoutClose,
        Command::LayoutSplit {
            direction: navo_core::Direction::Right,
        },
    ];
    for (index, command) in commands.iter().enumerate() {
        view.update(cx, |view, cx| {
            view.browser.command(command).unwrap();
            cx.notify();
        });
        settle(cx, &view);
        wait(cx, 2000);
        assert_eq!(saves.get(), index + 1, "{command:?}");
    }
    view.update(cx, |view, cx| {
        let panes = view.panes();
        view.browser
            .move_tab(TabMove {
                from: panes[0],
                index: 0,
                to: panes[1],
                at: 0,
                copy: false,
            })
            .unwrap();
        cx.notify();
    });
    wait(cx, 2000);
    assert_eq!(saves.get(), commands.len() + 1);
}
