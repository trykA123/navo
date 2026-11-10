//! Tests for the change map: tick positions, merging, the viewport box, click and drag to a row.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, Subscription, TestAppContext};

use super::geometry::{MIN_HEIGHT, tick_of};
use super::*;
use crate::test_pointer::{dispatch, down, moved, up};
use crate::test_window::open;

fn hunk(rows: Range<u32>, kind: HunkKind) -> MapHunk {
    MapHunk { rows, kind }
}

#[test]
fn ticks_sit_at_their_row_fraction() {
    let hunks = [
        hunk(100..200, HunkKind::Changed),
        hunk(500..600, HunkKind::Added),
    ];
    let t = ticks(&hunks, 1000, 500.);
    assert_eq!(t.len(), 2);
    assert_eq!((t[0].top, t[0].height), (50., 50.));
    assert_eq!((t[1].top, t[1].height), (250., 50.));
    assert_eq!(t[1].kind, HunkKind::Added);
}

#[test]
fn a_tiny_hunk_still_gets_the_minimum_height() {
    let t = ticks(&[hunk(10..11, HunkKind::Deleted)], 1_000_000, 500.);
    assert_eq!(t[0].height, MIN_HEIGHT);
}

#[test]
fn the_last_tick_stays_inside_the_strip() {
    let t = ticks(&[hunk(999..1000, HunkKind::Changed)], 1000, 100.);
    assert!(t[0].top + t[0].height <= 100.);
}

#[test]
fn ticks_less_than_one_pixel_apart_merge() {
    let hunks = [
        hunk(0..1, HunkKind::Added),
        hunk(2..3, HunkKind::Added),
        hunk(600_000..600_010, HunkKind::Deleted),
    ];
    let t = ticks(&hunks, 1_000_000, 500.);
    assert_eq!(t.len(), 2);
    assert_eq!(t[0].hunks, 0..2);
    assert_eq!(t[0].kind, HunkKind::Added);
    assert_eq!(t[1].hunks, 2..3);
    assert_eq!(tick_of(&t, 1), Some(0));
    assert_eq!(tick_of(&t, 2), Some(1));
    assert_eq!(tick_of(&t, 3), None);
}

#[test]
fn a_merge_of_mixed_kinds_is_changed() {
    let hunks = [hunk(0..1, HunkKind::Added), hunk(1..2, HunkKind::Deleted)];
    let t = ticks(&hunks, 1_000_000, 500.);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].kind, HunkKind::Changed);
}

#[test]
fn no_hunks_and_no_rows_give_no_ticks() {
    assert!(ticks(&[], 100, 500.).is_empty());
    assert!(ticks(&[], 0, 500.).is_empty());
}

#[test]
fn the_viewport_box_follows_the_visible_rows() {
    assert_eq!(viewport_box(&(250..500), 1000, 400.), (100., 100.));
    let (top, height) = viewport_box(&(0..1), 1_000_000, 400.);
    assert_eq!((top, height), (0., MIN_HEIGHT));
    let (top, height) = viewport_box(&(990..1100), 1000, 400.);
    assert!(top + height <= 400.);
}

#[test]
fn a_click_maps_to_a_row_and_clamps() {
    assert_eq!(row_at(0., 1000, 500.), 0);
    assert_eq!(row_at(250., 1000, 500.), 500);
    assert_eq!(row_at(499.9, 1000, 500.), 999);
    assert_eq!(row_at(-20., 1000, 500.), 0);
    assert_eq!(row_at(900., 1000, 500.), 999);
    assert_eq!(row_at(10., 0, 500.), 0);
}

struct Host {
    map: Entity<ChangeMap>,
    events: Rc<RefCell<Vec<MapEvent>>>,
    _events: Subscription,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(22.)).h(px(500.)).child(self.map.clone())
    }
}

fn host(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Host>) {
    let (window, host) = open(cx, |_, cx| {
        let hunks: Arc<[MapHunk]> = Arc::new([hunk(100..200, HunkKind::Changed)]);
        let map = cx.new(|_| ChangeMap::new(hunks, 1000));
        let events = Rc::new(RefCell::new(Vec::new()));
        let seen = events.clone();
        let subscription = cx.subscribe(&map, move |_, event: &MapEvent, _| {
            seen.borrow_mut().push(*event);
        });
        Host {
            map,
            events,
            _events: subscription,
        }
    });
    for _ in 0..2 {
        cx.update_window(window, |_, window, cx| window.render_frame(cx))
            .expect("window");
        cx.run_until_parked();
    }
    (window, host)
}

#[gpui_kit::test]
fn a_click_emits_a_jump_to_the_row_under_the_pointer(cx: &mut TestAppContext) {
    let (window, host) = host(cx);
    let at = point(px(10.), px(250.));
    dispatch(
        cx,
        window,
        down(MouseButton::Left, at, Modifiers::default()),
    );
    dispatch(cx, window, up(MouseButton::Left, at, Modifiers::default()));
    assert_eq!(
        host.read_with(cx, |host, _| host.events.borrow().clone()),
        vec![MapEvent::Jump(500)]
    );
}

#[gpui_kit::test]
fn a_drag_keeps_jumping_and_a_move_without_a_press_does_not(cx: &mut TestAppContext) {
    let (window, host) = host(cx);
    let mods = Modifiers::default();
    dispatch(cx, window, moved(None, point(px(10.), px(50.)), mods));
    dispatch(
        cx,
        window,
        down(MouseButton::Left, point(px(10.), px(100.)), mods),
    );
    let left = Some(MouseButton::Left);
    dispatch(cx, window, moved(left, point(px(10.), px(200.)), mods));
    dispatch(
        cx,
        window,
        up(MouseButton::Left, point(px(10.), px(200.)), mods),
    );
    dispatch(cx, window, moved(left, point(px(10.), px(300.)), mods));
    assert_eq!(
        host.read_with(cx, |host, _| host.events.borrow().clone()),
        vec![MapEvent::Jump(200), MapEvent::Jump(400)]
    );
}
