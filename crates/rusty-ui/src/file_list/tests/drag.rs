use std::path::PathBuf;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Context, Entity, ExternalPaths, FileDropEvent, FocusHandle,
    InputEvent as _, IntoElement, Modifiers, MouseButton, ParentElement, Pixels, Point, Render,
    Styled, TestAppContext, Window, div, point, px,
};
use rusty_core::{Command, CursorMove, PaneId, SpanMode};

use super::mouse::{LAST_ROW, below, gap_of, icon_of, row_bounds};
use super::{props, sample_rows};
use crate::drag::{DragBadge, DragState, FileDrag};
use crate::file_list::{DropEvent, DropPayload, DropSpot, FileList, FileListProps};
use crate::test_pointer::{self, ctrl, dispatch, down, moved, shift, up};
use crate::test_window::open;

const PANE: PaneId = PaneId::new(7);
const SRC: usize = 0;
const CARGO: usize = 2;
const LAYOUT: usize = 3;

struct Host {
    props: FileListProps,
    focus: FocusHandle,
    wired: bool,
    commands: Vec<Command>,
    starts: Vec<FileDrag>,
    drops: Vec<DropEvent>,
    hovers: Vec<Option<DropSpot>>,
}

fn badge(cx: &mut gpui_kit::App) -> Entity<DragBadge> {
    let state = cx.new(|_| DragState {
        count: 1,
        first_name: "Cargo.toml".into(),
        hover: None,
        keys_line: true,
    });
    cx.new(|cx| DragBadge::new(state, cx))
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let list = FileList::new("left", self.props.clone())
            .track_focus(&self.focus)
            .on_command(cx.listener(|this, command: &Command, _, _| {
                this.commands.push(command.clone());
            }));
        let list = if self.wired {
            let host = cx.entity().downgrade();
            list.pane(PANE)
                .on_drag_start(move |drag, _, cx| {
                    let _ = host.update(cx, |host, _| host.starts.push(drag.clone()));
                    badge(cx)
                })
                .on_drop(cx.listener(|this, event: &DropEvent, _, _| {
                    this.drops.push(event.clone());
                }))
                .on_hover(cx.listener(|this, spot: &Option<DropSpot>, _, _| {
                    this.hovers.push(spot.clone());
                }))
        } else {
            list
        };
        div()
            .size_full()
            .child(div().h(px(480.)).flex().flex_col().child(list))
    }
}

fn open_host(cx: &mut TestAppContext, wired: bool) -> (AnyWindowHandle, Entity<Host>) {
    let (window, host) = open(cx, |_, cx| Host {
        props: props(sample_rows()),
        focus: cx.focus_handle(),
        wired,
        commands: Vec::new(),
        starts: Vec::new(),
        drops: Vec::new(),
        hovers: Vec::new(),
    });
    cx.update_window(window, |_, window, cx| {
        let focus = host.read(cx).focus.clone();
        window.focus(&focus, cx);
        window.render_frame(cx);
    })
    .expect("window");
    (window, host)
}

fn centre(cx: &mut TestAppContext, window: AnyWindowHandle, row: usize) -> Point<Pixels> {
    row_bounds(cx, window, row).center()
}

fn name_of(cx: &mut TestAppContext, window: AnyWindowHandle, row: usize) -> Point<Pixels> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let bounds = window
            .within("left")
            .within(("row", row))
            .find("grip")
            .bounds();
        point(bounds.right() - px(4.), bounds.center().y)
    })
    .expect("window")
}

fn left_drag(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    modifiers: Modifiers,
    path: &[Point<Pixels>],
) {
    test_pointer::drag(cx, window, (MouseButton::Left, modifiers), path);
}

fn release(cx: &mut TestAppContext, window: AnyWindowHandle, at: Point<Pixels>, mods: Modifiers) {
    dispatch(cx, window, up(MouseButton::Left, at, mods));
}

fn has_drag(cx: &mut TestAppContext) -> bool {
    cx.update(|cx| cx.has_active_drag())
}

fn read<T>(host: &Entity<Host>, cx: &mut TestAppContext, f: impl Fn(&Host) -> T) -> T {
    host.read_with(cx, |host, _| f(host))
}

fn nudge(at: Point<Pixels>) -> Point<Pixels> {
    point(at.x + px(12.), at.y)
}

fn cursor(row: usize) -> Command {
    Command::PaneCursorMove {
        to: CursorMove::Row(row as u64),
    }
}

fn internal(row: usize) -> DropPayload {
    DropPayload::Internal(FileDrag {
        pane: PANE,
        row: row as u64,
        name: sample_rows()[row].name.to_text().into(),
    })
}

#[gpui_kit::test]
fn a_left_drag_from_the_icon_starts_a_file_drag_for_that_row(cx: &mut TestAppContext) {
    let (window, host) = open_host(cx, true);
    let from = icon_of(cx, window, CARGO);
    left_drag(cx, window, Modifiers::default(), &[from, nudge(from)]);
    assert!(has_drag(cx));
    let starts = read(&host, cx, |h| h.starts.clone());
    assert_eq!(
        starts,
        [FileDrag {
            pane: PANE,
            row: 2,
            name: "Cargo.toml".into()
        }]
    );
    let is_lasso = cx
        .update_window(window, |_, window, _| {
            window.within("left").try_find("lasso").is_some()
        })
        .expect("window");
    assert!(!is_lasso);
}

#[gpui_kit::test]
fn a_drop_on_a_folder_row_names_that_row(cx: &mut TestAppContext) {
    let (window, host) = open_host(cx, true);
    let (from, to) = (name_of(cx, window, CARGO), centre(cx, window, SRC));
    left_drag(cx, window, Modifiers::default(), &[from, nudge(from), to]);
    assert_eq!(
        read(&host, cx, |h| h.hovers.last().cloned()),
        Some(Some(DropSpot::Row(0, "src".into())))
    );
    release(cx, window, to, Modifiers::default());
    let expected = DropEvent {
        pane: PANE,
        spot: DropSpot::Row(0, "src".into()),
        payload: internal(CARGO),
    };
    assert_eq!(read(&host, cx, |h| h.drops.clone()), [expected]);
    assert!(!has_drag(cx));
}

#[gpui_kit::test]
fn a_drop_on_a_file_row_or_below_the_rows_targets_the_pane_folder(cx: &mut TestAppContext) {
    let (window, host) = open_host(cx, true);
    let from = icon_of(cx, window, CARGO);
    let on_file = centre(cx, window, LAYOUT);
    left_drag(
        cx,
        window,
        Modifiers::default(),
        &[from, nudge(from), on_file],
    );
    assert_eq!(
        read(&host, cx, |h| h.hovers.last().cloned()),
        Some(Some(DropSpot::Folder))
    );
    release(cx, window, on_file, Modifiers::default());
    let to = below(cx, window);
    left_drag(cx, window, Modifiers::default(), &[from, nudge(from), to]);
    release(cx, window, to, Modifiers::default());
    let spots: Vec<_> = read(&host, cx, |h| {
        h.drops.iter().map(|d| d.spot.clone()).collect()
    });
    assert_eq!(spots, [DropSpot::Folder, DropSpot::Folder]);
}

#[gpui_kit::test]
fn hovering_outside_the_list_reports_no_spot(cx: &mut TestAppContext) {
    let (window, host) = open_host(cx, true);
    let from = icon_of(cx, window, CARGO);
    let outside = point(from.x, px(560.));
    left_drag(
        cx,
        window,
        Modifiers::default(),
        &[from, nudge(from), outside],
    );
    assert_eq!(read(&host, cx, |h| h.hovers.last().cloned()), Some(None));
    release(cx, window, outside, Modifiers::default());
    assert!(read(&host, cx, |h| h.drops.is_empty()));
}

#[gpui_kit::test]
fn files_dropped_from_outside_on_a_folder_row_arrive_as_external(cx: &mut TestAppContext) {
    let (window, host) = open_host(cx, true);
    let at = centre(cx, window, SRC);
    let paths = ExternalPaths([PathBuf::from("notes.txt")].into_iter().collect());
    let entered = FileDropEvent::Entered {
        position: at,
        paths: paths.clone(),
    };
    dispatch(cx, window, entered.to_platform_input());
    let submit = FileDropEvent::Submit { position: at };
    dispatch(cx, window, submit.to_platform_input());
    let expected = DropEvent {
        pane: PANE,
        spot: DropSpot::Row(0, "src".into()),
        payload: DropPayload::External(paths),
    };
    assert_eq!(read(&host, cx, |h| h.drops.clone()), [expected]);
    assert!(read(&host, cx, |h| h.commands.is_empty()));
}

#[gpui_kit::test]
fn without_a_drag_start_handler_there_is_no_drag(cx: &mut TestAppContext) {
    let (window, host) = open_host(cx, false);
    let from = icon_of(cx, window, CARGO);
    left_drag(cx, window, Modifiers::default(), &[from, nudge(from)]);
    assert!(!has_drag(cx));
    release(cx, window, nudge(from), Modifiers::default());
    assert_eq!(read(&host, cx, |h| h.commands.clone()), [cursor(CARGO)]);
}

#[gpui_kit::test]
fn a_press_on_the_gap_still_draws_a_lasso(cx: &mut TestAppContext) {
    let (window, host) = open_host(cx, true);
    let (from, to) = (gap_of(cx, window, 1), centre(cx, window, LAYOUT));
    left_drag(cx, window, Modifiers::default(), &[from, to]);
    assert!(!has_drag(cx));
    release(cx, window, to, Modifiers::default());
    let extend = Command::SelectionExtend {
        to: CursorMove::Row(3),
        mode: SpanMode::Mark,
    };
    let commands = read(&host, cx, |h| h.commands.clone());
    assert_eq!(commands.last(), Some(&extend));
    assert!(read(&host, cx, |h| h.starts.is_empty()));
}

#[gpui_kit::test]
fn a_right_drag_over_the_names_still_marks_a_range(cx: &mut TestAppContext) {
    let (window, host) = open_host(cx, true);
    let path = [1, 2, 3].map(|row| name_of(cx, window, row));
    test_pointer::drag(
        cx,
        window,
        (MouseButton::Right, Modifiers::default()),
        &path,
    );
    dispatch(
        cx,
        window,
        up(MouseButton::Right, path[2], Modifiers::default()),
    );
    assert!(!has_drag(cx));
    let commands = read(&host, cx, |h| h.commands.clone());
    let extend = |row| Command::SelectionExtend {
        to: CursorMove::Row(row),
        mode: SpanMode::Mark,
    };
    assert_eq!(commands[commands.len() - 2..], [extend(2), extend(3)]);
    assert!(!commands.contains(&Command::SelectionToggle));
    assert!(read(&host, cx, |h| h.starts.is_empty()));
}

#[gpui_kit::test]
fn a_ctrl_drag_released_on_its_row_before_a_redraw_does_not_toggle(cx: &mut TestAppContext) {
    let (window, host) = open_host(cx, true);
    let from = icon_of(cx, window, CARGO);
    dispatch(cx, window, down(MouseButton::Left, from, ctrl()));
    cx.update_window(window, |_, window, cx| {
        window.dispatch_event(moved(Some(MouseButton::Left), nudge(from), ctrl()), cx);
        window.dispatch_event(up(MouseButton::Left, nudge(from), ctrl()), cx);
    })
    .expect("window");
    cx.run_until_parked();
    assert_eq!(read(&host, cx, |h| h.starts.len()), 1);
    assert!(!read(&host, cx, |h| h
        .commands
        .contains(&Command::SelectionToggle)));
}

#[gpui_kit::test]
fn clicks_after_a_cancelled_drag_move_the_cursor_and_toggle(cx: &mut TestAppContext) {
    let (window, host) = open_host(cx, true);
    let from = icon_of(cx, window, CARGO);
    let to = centre(cx, window, LAST_ROW);
    left_drag(cx, window, Modifiers::default(), &[from, nudge(from), to]);
    let stopped = cx
        .update_window(window, |_, window, cx| cx.stop_active_drag(window))
        .expect("window");
    assert!(stopped);
    release(cx, window, to, Modifiers::default());
    let before = read(&host, cx, |h| h.commands.len());
    let (four, one) = (name_of(cx, window, 4), name_of(cx, window, 1));
    test_pointer::click(cx, window, (four, Modifiers::default()));
    test_pointer::click(cx, window, (one, ctrl()));
    let commands = read(&host, cx, |h| h.commands[before..].to_vec());
    assert_eq!(commands, [cursor(4), cursor(1), Command::SelectionToggle]);
    assert!(read(&host, cx, |h| h.drops.is_empty()));
}

#[gpui_kit::test]
fn ctrl_and_shift_clicks_on_names_still_mark(cx: &mut TestAppContext) {
    let (window, host) = open_host(cx, true);
    let one = name_of(cx, window, 1);
    test_pointer::click(cx, window, (one, ctrl()));
    let at = name_of(cx, window, LAYOUT);
    dispatch(cx, window, down(MouseButton::Left, at, shift()));
    dispatch(cx, window, moved(Some(MouseButton::Left), at, shift()));
    dispatch(cx, window, up(MouseButton::Left, at, shift()));
    let extend = Command::SelectionExtend {
        to: CursorMove::Row(3),
        mode: SpanMode::Mark,
    };
    let commands = read(&host, cx, |h| h.commands.clone());
    assert_eq!(commands, [cursor(1), Command::SelectionToggle, extend]);
    assert!(!has_drag(cx));
}
