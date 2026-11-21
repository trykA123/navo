//! Tests of the start screen's row click: one click selects, a double click opens.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent,
    TestAppContext, VisualTestContext,
};
use navo_compare_profile::recent::{RowKind, StartRow, StartRows};

use crate::start::{StartEvent, StartScreen};
use crate::test_window::open;

/// One recent row with the given id.
fn row(id: &str) -> StartRow {
    StartRow {
        id: id.into(),
        name: None,
        left: "a".into(),
        right: "b".into(),
        kind: RowKind::Paths,
        age: "now".into(),
    }
}

/// Opens a start screen with three rows and returns the window, view and a log of its events.
fn setup(
    cx: &mut TestAppContext,
) -> (
    AnyWindowHandle,
    gpui_kit::Entity<StartScreen>,
    Rc<RefCell<Vec<StartEvent>>>,
) {
    let rows = StartRows {
        recent: vec![row("r1"), row("r2")],
        named: vec![row("n1")],
    };
    let (window, view) = open(cx, move |window, cx| {
        StartScreen::new(rows, std::env::temp_dir(), window, cx)
    });
    let log = Rc::new(RefCell::new(Vec::new()));
    let sink = log.clone();
    cx.update(|cx| {
        cx.subscribe(&view, move |_, event: &StartEvent, _| {
            sink.borrow_mut().push(event.clone());
        })
        .detach();
    });
    (window, view, log)
}

/// Presses and releases the left button on row 2 with the given click count.
fn click_row_two(cx: &mut TestAppContext, window: AnyWindowHandle, click_count: usize) {
    let at = cx
        .update_window(window, |_, w, _| {
            w.find(("start-row", 2usize)).bounds().center()
        })
        .expect("window");
    let mut vcx = VisualTestContext::from_window(window, cx);
    vcx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::default(),
        click_count,
        first_mouse: false,
    });
    vcx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::default(),
        click_count,
    });
}

#[gpui_kit::test]
fn clicking_a_row_only_selects_it(cx: &mut TestAppContext) {
    let (window, view, log) = setup(cx);
    click_row_two(cx, window, 1);
    assert_eq!(view.read_with(cx, |screen, _| screen.selected), Some(2));
    assert!(log.borrow().is_empty());
}

#[gpui_kit::test]
fn double_clicking_a_row_opens_it(cx: &mut TestAppContext) {
    let (window, view, log) = setup(cx);
    click_row_two(cx, window, 2);
    assert_eq!(view.read_with(cx, |screen, _| screen.selected), Some(2));
    assert!(matches!(
        log.borrow().as_slice(),
        [StartEvent::Open { id }] if id == "n1"
    ));
}
