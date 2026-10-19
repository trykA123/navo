use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{DisplayId, TestAppContext, point};
use rusty_core::AppState;
use rusty_core::placement::{
    Monitor, MonitorLocalLogicalRect, MonitorSetup, PhysicalRect, Placement,
};

use super::super::placement::DisplayScales;
use super::super::start::main_window_options;
use super::super::{FerriteWindow, StartOptions};
use super::{close_in, fake, open_options, options_for, two_panes};
use crate::prelude::*;

const TEST_DISPLAY: PhysicalRect = PhysicalRect {
    x: 0,
    y: 0,
    width: 1920,
    height: 1080,
};

fn test_setup() -> MonitorSetup {
    MonitorSetup::new(vec![Monitor {
        bounds: TEST_DISPLAY,
        work_area: TEST_DISPLAY,
        scale: 1.0,
        primary: true,
    }])
    .expect("setup")
}

fn placed(state: &mut AppState, maximized: bool) {
    let setup = test_setup();
    let placement = Placement {
        setup: setup.fingerprint(),
        bounds: MonitorLocalLogicalRect {
            x: 200.,
            y: 100.,
            width: 1000.,
            height: 640.,
        },
        monitor: TEST_DISPLAY,
        maximized,
    };
    state.remember_placement(&setup, placement);
}

fn start(cx: &mut TestAppContext, options: StartOptions) -> WindowBounds {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::tokens::apply(crate::tokens::Mode::Light, cx);
        let window_options = main_window_options(&options.state, cx);
        let (window, _) = gpui_kit::open_window(window_options, cx, |window, cx| {
            cx.new(|cx| FerriteWindow::new(options, window, cx))
        })
        .expect("open window");
        window
            .update(cx, |_, window, _| window.window_bounds())
            .expect("window")
    })
}

#[gpui_kit::test]
fn a_session_with_a_placement_opens_the_window_at_its_bounds(cx: &mut TestAppContext) {
    let fs = fake();
    let mut state = two_panes(&fs);
    placed(&mut state, false);
    let bounds = start(cx, options_for(fs, state));
    let expected = Bounds {
        origin: point(px(200.), px(100.)),
        size: size(px(1000.), px(640.)),
    };
    assert_eq!(bounds, WindowBounds::Windowed(expected));
}

#[gpui_kit::test]
fn a_placement_opens_on_its_display(cx: &mut TestAppContext) {
    let fs = fake();
    let mut state = two_panes(&fs);
    placed(&mut state, true);
    let options = cx.update(|cx| main_window_options(&state, cx));
    assert_eq!(options.display_id, Some(DisplayId::new(1)));
    assert!(matches!(
        options.window_bounds,
        Some(WindowBounds::Maximized(_))
    ));
}

#[gpui_kit::test]
fn an_unreadable_display_list_falls_back_to_the_centred_window(cx: &mut TestAppContext) {
    let fs = fake();
    let mut state = two_panes(&fs);
    placed(&mut state, false);
    let (options, centred) = cx.update(|cx| {
        cx.set_global(DisplayScales(Rc::new(|_| Some(f32::NAN))));
        let centred = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        (main_window_options(&state, cx), centred)
    });
    assert_eq!(options.display_id, None);
    assert_eq!(options.window_bounds, Some(WindowBounds::Windowed(centred)));
}

#[gpui_kit::test]
fn closing_the_window_saves_its_placement(cx: &mut TestAppContext) {
    let fs = fake();
    let saved: Rc<RefCell<Option<AppState>>> = Rc::default();
    let sink = saved.clone();
    let options = StartOptions {
        on_save: Rc::new(move |state: &AppState| *sink.borrow_mut() = Some(state.clone())),
        ..options_for(fs.clone(), two_panes(&fs))
    };
    let (window, view) = open_options(cx, options);
    assert!(close_in(cx, window, &view));
    let state = saved.borrow_mut().take().expect("saved session");
    let placements = state
        .window(state.focused_window())
        .map(|window| window.placement().to_vec())
        .unwrap_or_default();
    let expected = Placement {
        setup: test_setup().fingerprint(),
        bounds: MonitorLocalLogicalRect {
            x: 0.,
            y: 0.,
            width: 1000.,
            height: 600.,
        },
        monitor: TEST_DISPLAY,
        maximized: false,
    };
    assert_eq!(placements, vec![expected]);
}
