use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, Point, TestAppContext, VisualTestContext,
    WindowBounds, WindowOptions, px, size,
};
use rusty_core::{AppState, Command, Direction, OpKind, apply};
use rusty_fs::FakeFs;
use rusty_fs::Provider;

use super::{FerriteWindow, StartOptions};
use crate::browser::Operations;
use crate::tokens::{self, Mode};

fn two_panes(fs: &FakeFs) -> AppState {
    let mut state = AppState::new(fs.location("home"));
    let _ = apply(
        &mut state,
        &Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    state
}

fn fake() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    fs.add_file("home/docs/report.txt", 7);
    fs.add_file("home/a.txt", 3);
    fs.add_folder("dest");
    Arc::new(fs)
}

fn open(
    cx: &mut TestAppContext,
    saves: Rc<Cell<usize>>,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let fs = fake();
    let state = two_panes(&fs);
    open_with(cx, fs, state, saves)
}

fn open_with(
    cx: &mut TestAppContext,
    fs: Arc<FakeFs>,
    state: AppState,
    saves: Rc<Cell<usize>>,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let options = StartOptions {
        state,
        provider: fs,
        on_save: Rc::new(move |_| saves.set(saves.get() + 1)),
        mode: Mode::Light,
        operations: Operations::unlogged(),
        launcher: Arc::new(|_| {}),
    };
    let handle = cx.update(|cx| {
        gpui_kit::init(cx);
        tokens::apply(Mode::Light, cx);
        crate::file_list::bind_keys(cx);
        crate::confirm_dialog::bind_keys(cx);
        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: size(px(1000.), px(600.)),
            })),
            ..Default::default()
        };
        gpui_kit::open_window(window_options, cx, |window, cx| {
            cx.new(|cx| FerriteWindow::new(options, window, cx))
        })
        .expect("open window")
    });
    let view = handle.1.clone();
    settle(cx, &view);
    handle
}

fn settle(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        view.update(cx, |view, cx| view.wake(cx));
        cx.run_until_parked();
        let loading = view.read_with(cx, |view, _| {
            view.browser.loading() || view.browser.running()
        });
        if !loading || Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[gpui_kit::test]
fn both_panes_show_their_rows(cx: &mut TestAppContext) {
    let (window, view) = open(cx, Rc::default());
    let panes = view.read_with(cx, |view, _| view.panes());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        for pane in &panes {
            let list = window.within(("list", pane.get()));
            assert!(list.find(("row", 0usize)).visible());
        }
    })
    .expect("window");
}

#[gpui_kit::test]
fn tab_switches_the_focused_pane(cx: &mut TestAppContext) {
    let (window, view) = open(cx, Rc::default());
    let before = view.read_with(cx, |view, _| view.browser.state().focused_pane());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.press("tab", cx);
    })
    .expect("window");
    let after = view.read_with(cx, |view, _| view.browser.state().focused_pane());
    assert_ne!(before, after);
}

#[gpui_kit::test]
fn closing_the_window_saves_the_session(cx: &mut TestAppContext) {
    let saves = Rc::new(Cell::new(0));
    let (window, _) = open(cx, saves.clone());
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.simulate_close();
    assert_eq!(saves.get(), 1);
}

#[test]
fn every_system_appearance_maps_to_a_theme_mode() {
    use gpui_kit::WindowAppearance as A;
    assert_eq!(super::mode_of(A::Light), Mode::Light);
    assert_eq!(super::mode_of(A::VibrantLight), Mode::Light);
    assert_eq!(super::mode_of(A::Dark), Mode::Dark);
    assert_eq!(super::mode_of(A::VibrantDark), Mode::Dark);
}

fn home_beside_dest(fs: &FakeFs) -> AppState {
    let mut state = two_panes(fs);
    let location = fs.location("dest");
    let _ = apply(&mut state, &Command::PaneNavigate { location });
    let dest = state.focused_pane();
    for direction in Direction::ALL {
        if state.focused_pane() != dest {
            break;
        }
        let _ = apply(&mut state, &Command::LayoutFocus { direction });
    }
    state
}

fn press(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<FerriteWindow>,
    key: &str,
) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.press(key, cx);
    })
    .expect("window");
    settle(cx, view);
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    cx.run_until_parked();
}

#[gpui_kit::test]
fn f5_copies_the_cursor_row_into_the_other_pane(cx: &mut TestAppContext) {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let (window, view) = open_with(cx, fs.clone(), state, Rc::default());
    press(cx, window, &view, "f5");
    let kind = view.read_with(cx, |view, _| view.browser.pending().map(|p| p.plan.kind));
    assert_eq!(kind, Some(OpKind::Copy));
    press(cx, window, &view, "enter");
    settle(cx, &view);
    assert!(fs.metadata(&fs.location("dest/docs/report.txt")).is_ok());
    assert!(fs.metadata(&fs.location("home/docs/report.txt")).is_ok());
}

#[gpui_kit::test]
fn shift_delete_asks_with_danger_and_escape_deletes_nothing(cx: &mut TestAppContext) {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let (window, view) = open_with(cx, fs.clone(), state, Rc::default());
    press(cx, window, &view, "shift-delete");
    let danger = view.read_with(cx, |view, _| {
        view.browser
            .pending()
            .map(|p| super::operate::confirm_props(p).danger)
    });
    assert_eq!(danger, Some(true));
    press(cx, window, &view, "escape");
    let pending = view.read_with(cx, |view, _| view.browser.pending().is_some());
    assert!(!pending);
    assert!(view.read_with(cx, |view, _| view.browser.jobs().is_empty()));
    assert!(fs.metadata(&fs.location("home/docs")).is_ok());
}
