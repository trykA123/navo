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
        crate::prompt_dialog::bind_keys(cx);
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

#[gpui_kit::test]
fn a_finished_job_row_goes_away_after_a_moment(cx: &mut TestAppContext) {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let (window, view) = open_with(cx, fs.clone(), state, Rc::default());
    press(cx, window, &view, "f5");
    press(cx, window, &view, "enter");
    settle(cx, &view);
    assert_eq!(view.read_with(cx, |view, _| view.browser.jobs().len()), 1);
    cx.executor().advance_clock(Duration::from_secs(5));
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.browser.jobs().is_empty()));
}

fn shown(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Vec<String> {
    view.read_with(cx, |view, _| {
        view.browser
            .state()
            .active_tab()
            .listing()
            .map(|l| l.entries().iter().map(|e| e.name.clone()).collect())
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn typing_in_a_pane_filters_it_and_escape_brings_everything_back(cx: &mut TestAppContext) {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let (window, view) = open_with(cx, fs, state, Rc::default());
    press(cx, window, &view, "d");
    assert_eq!(shown(cx, &view), ["docs"]);
    cx.update_window(window, |_, window, _| {
        assert!(window.find("filter").visible());
    })
    .expect("window");
    press(cx, window, &view, "escape");
    assert_eq!(shown(cx, &view), ["docs", "a.txt"]);
}

fn type_in(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<FerriteWindow>,
    text: &str,
) {
    cx.update_window(window, |_, window, cx| window.input(text, cx))
        .expect("window");
    settle(cx, view);
}

fn prompt_open(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> bool {
    view.read_with(cx, |view, _| view.prompt.is_some())
}

#[gpui_kit::test]
fn f2_renames_the_cursor_row(cx: &mut TestAppContext) {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let (window, view) = open_with(cx, fs.clone(), state, Rc::default());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f2");
    assert!(prompt_open(cx, &view));
    type_in(cx, window, &view, "b");
    press(cx, window, &view, "enter");
    settle(cx, &view);
    assert!(!prompt_open(cx, &view));
    assert!(fs.metadata(&fs.location("home/b.txt")).is_ok());
    assert!(fs.metadata(&fs.location("home/a.txt")).is_err());
}

#[gpui_kit::test]
fn f7_creates_a_folder_named_by_default(cx: &mut TestAppContext) {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let (window, view) = open_with(cx, fs.clone(), state, Rc::default());
    press(cx, window, &view, "f7");
    press(cx, window, &view, "enter");
    settle(cx, &view);
    assert!(fs.metadata(&fs.location("home/New folder")).is_ok());
}

#[gpui_kit::test]
fn a_bad_name_keeps_the_prompt_open_with_its_error(cx: &mut TestAppContext) {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let (window, view) = open_with(cx, fs.clone(), state, Rc::default());
    press(cx, window, &view, "f7");
    type_in(cx, window, &view, "docs");
    press(cx, window, &view, "enter");
    assert!(prompt_open(cx, &view));
    cx.update_window(window, |_, window, _| {
        assert!(window.find("error").visible());
    })
    .expect("window");
    press(cx, window, &view, "escape");
    assert!(!prompt_open(cx, &view));
}

#[test]
fn the_rename_selection_stops_before_the_extension() {
    use super::prompt::stem_end;
    assert_eq!(stem_end("report.txt"), 6);
    assert_eq!(stem_end("archive.tar.gz"), 11);
    assert_eq!(stem_end(".gitignore"), 10);
    assert_eq!(stem_end("Makefile"), 8);
}

#[gpui_kit::test]
fn letters_do_not_filter_while_a_prompt_is_open(cx: &mut TestAppContext) {
    use crate::file_list::FilterEdit;
    let fs = fake();
    let state = home_beside_dest(&fs);
    let (window, view) = open_with(cx, fs, state, Rc::default());
    press(cx, window, &view, "f7");
    view.update(cx, |view, _| view.filter(FilterEdit::Push('d')));
    let tab = view.read_with(cx, |view, _| view.browser.state().active_tab().id());
    assert!(view.read_with(cx, |view, _| view.browser.filter_text(tab).is_none()));
}

#[gpui_kit::test]
fn plus_marks_by_pattern_and_star_inverts(cx: &mut TestAppContext) {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let (window, view) = open_with(cx, fs, state, Rc::default());
    press(cx, window, &view, "+");
    type_in(cx, window, &view, "*.txt");
    press(cx, window, &view, "enter");
    let marked = |cx: &mut TestAppContext| {
        view.read_with(cx, |view, _| {
            view.browser.state().active_tab().selection().marked()
        })
    };
    assert_eq!(marked(cx), [1]);
    press(cx, window, &view, "*");
    assert!(marked(cx).is_empty());
}

#[gpui_kit::test]
fn an_empty_pattern_keeps_the_prompt_open(cx: &mut TestAppContext) {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let (window, view) = open_with(cx, fs, state, Rc::default());
    press(cx, window, &view, "-");
    press(cx, window, &view, "backspace");
    press(cx, window, &view, "enter");
    assert!(prompt_open(cx, &view));
}

#[gpui_kit::test]
fn ctrl_f6_sorts_the_pane_by_size(cx: &mut TestAppContext) {
    use crate::file_list::SortBy;
    let fs = fake();
    let state = home_beside_dest(&fs);
    let (window, view) = open_with(cx, fs, state, Rc::default());
    press(cx, window, &view, "ctrl-f6");
    let tab = view.read_with(cx, |view, _| view.browser.state().active_tab().id());
    let key = view.read_with(cx, |view, _| view.browser.sort_key(tab));
    assert_eq!(key.by, SortBy::Size);
}
