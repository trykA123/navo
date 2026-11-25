use gpui_kit::{DisplayId, point};
use navo_core::placement::{
    Monitor, MonitorLocalLogicalRect, MonitorSetup, PhysicalRect, Placement,
};

use super::{Screen, Shown, first_place, fit_on, keep_on, opening, opening_saved, placement};
use crate::prelude::*;

fn logical(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
    Bounds {
        origin: point(px(x), px(y)),
        size: size(px(width), px(height)),
    }
}

fn screen(id: u64, physical: [f32; 4], taskbar: f32, scale: f32) -> Screen {
    let [x, y, width, height] = physical.map(|value| value / scale);
    Screen {
        id: DisplayId::new(id),
        bounds: logical(x, y, width, height),
        work_area: logical(x, y, width, height - taskbar / scale),
        scale,
        primary: id == 1,
    }
}

fn office() -> Vec<Screen> {
    vec![
        screen(1, [0., 0., 2560., 1440.], 48., 1.5),
        screen(2, [-1920., 200., 1920., 1080.], 40., 1.0),
    ]
}

fn approx(left: MonitorLocalLogicalRect, right: MonitorLocalLogicalRect) -> bool {
    [
        (left.x, right.x),
        (left.y, right.y),
        (left.width, right.width),
        (left.height, right.height),
    ]
    .iter()
    .all(|(a, b)| (a - b).abs() < 0.01)
}

#[test]
fn a_window_on_the_scaled_monitor_is_stored_in_its_local_logical_pixels() {
    let shown = Shown {
        bounds: WindowBounds::Windowed(logical(100., 50., 1200., 700.)),
        display: Some(DisplayId::new(1)),
    };
    let (_, saved) = placement(&office(), shown).expect("placement");
    assert_eq!(
        saved.monitor,
        PhysicalRect {
            x: 0,
            y: 0,
            width: 2560,
            height: 1440
        }
    );
    let expected = MonitorLocalLogicalRect {
        x: 100.,
        y: 50.,
        width: 1200.,
        height: 700.,
    };
    assert!(approx(saved.bounds, expected), "{:?}", saved.bounds);
    assert!(!saved.maximized);
}

#[test]
fn a_window_on_the_offset_monitor_is_stored_relative_to_its_origin() {
    let shown = Shown {
        bounds: WindowBounds::Maximized(logical(-1800., 260., 900., 600.)),
        display: Some(DisplayId::new(2)),
    };
    let (_, saved) = placement(&office(), shown).expect("placement");
    assert_eq!(saved.monitor.x, -1920);
    assert_eq!(saved.monitor.y, 200);
    let expected = MonitorLocalLogicalRect {
        x: 120.,
        y: 60.,
        width: 900.,
        height: 600.,
    };
    assert!(approx(saved.bounds, expected), "{:?}", saved.bounds);
    assert!(saved.maximized);
}

#[test]
fn a_saved_placement_opens_on_the_same_display_with_the_same_bounds() {
    let screens = office();
    for (display, bounds) in [
        (1, logical(100., 50., 1200., 700.)),
        (2, logical(-1800., 260., 900., 600.)),
    ] {
        let shown = Shown {
            bounds: WindowBounds::Windowed(bounds),
            display: Some(DisplayId::new(display)),
        };
        let (_, saved) = placement(&screens, shown).expect("placement");
        let (id, opened) = opening(&screens, &[saved]).expect("opening");
        assert_eq!(id, DisplayId::new(display));
        let WindowBounds::Windowed(opened) = opened else {
            panic!("expected a normal window, got {opened:?}");
        };
        let close = |a: Pixels, b: Pixels| (a.as_f32() - b.as_f32()).abs() < 0.01;
        assert!(close(opened.origin.x, bounds.origin.x), "{opened:?}");
        assert!(close(opened.origin.y, bounds.origin.y), "{opened:?}");
        assert!(close(opened.size.width, bounds.size.width), "{opened:?}");
        assert!(close(opened.size.height, bounds.size.height), "{opened:?}");
    }
}

#[test]
fn a_maximized_placement_opens_maximized() {
    let screens = office();
    let shown = Shown {
        bounds: WindowBounds::Maximized(logical(-1800., 260., 900., 600.)),
        display: Some(DisplayId::new(2)),
    };
    let (_, saved) = placement(&screens, shown).expect("placement");
    let (_, opened) = opening(&screens, &[saved]).expect("opening");
    assert!(matches!(opened, WindowBounds::Maximized(_)), "{opened:?}");
}

#[test]
fn opening_rounds_to_whole_physical_pixels() {
    let screens = vec![screen(1, [0., 0., 2560., 1440.], 0., 1.25)];
    let shown = Shown {
        bounds: WindowBounds::Windowed(logical(10.3, 20.7, 1000.1, 600.5)),
        display: Some(DisplayId::new(1)),
    };
    let (_, saved) = placement(&screens, shown).expect("placement");
    let (_, opened) = opening(&screens, &[saved]).expect("opening");
    let bounds = opened.get_bounds();
    for value in [
        bounds.origin.x,
        bounds.origin.y,
        bounds.size.width,
        bounds.size.height,
    ] {
        let physical = value.as_f32() * 1.25;
        assert!((physical - physical.round()).abs() < 0.001, "{bounds:?}");
    }
}

#[test]
fn without_placements_the_window_opens_centred_on_the_primary_display() {
    let (id, opened) = opening(&office(), &[]).expect("opening");
    assert_eq!(id, DisplayId::new(1));
    let bounds = opened.get_bounds();
    assert_eq!(bounds.size.width, px(1280.));
    assert!(bounds.size.height < px(800.), "{bounds:?}");
    assert!(matches!(opened, WindowBounds::Windowed(_)), "{opened:?}");
}

#[test]
fn an_unusable_display_list_gives_no_opening_or_placement() {
    let mut broken = office();
    broken[1].scale = f32::NAN;
    let shown = Shown {
        bounds: WindowBounds::Windowed(logical(0., 0., 800., 600.)),
        display: Some(DisplayId::new(1)),
    };
    assert_eq!(opening(&[], &[]), None);
    assert_eq!(opening(&broken, &[]), None);
    assert_eq!(placement(&broken, shown), None);
}

#[test]
fn a_window_on_an_unknown_display_is_not_stored() {
    let shown = Shown {
        bounds: WindowBounds::Windowed(logical(0., 0., 800., 600.)),
        display: Some(DisplayId::new(9)),
    };
    assert_eq!(placement(&office(), shown), None);
}

#[test]
fn an_enlarged_window_in_a_display_corner_moves_back_onto_the_display() {
    let area = office()[1].work_area;
    let small = WindowBounds::Windowed(logical(-80., 1100., 300., 120.));
    let WindowBounds::Windowed(kept) = keep_on(crate::title_bar::at_least_minimum(small), area)
    else {
        panic!("windowed bounds stay windowed");
    };
    assert_eq!(kept.size, size(px(480.), px(360.)));
    assert!(kept.right() <= area.right() && kept.bottom() <= area.bottom());
    assert!(kept.left() >= area.left() && kept.top() >= area.top());
}

#[test]
fn a_window_inside_its_display_keeps_its_origin() {
    let area = office()[0].work_area;
    let inside = WindowBounds::Maximized(logical(100., 80., 900., 600.));
    assert_eq!(keep_on(inside, area), inside);
}

#[test]
fn opening_saved_returns_none_when_empty_or_offscreen() {
    assert_eq!(opening_saved(&office(), &[]), None);
    let gone = Monitor {
        bounds: PhysicalRect {
            x: 4000,
            y: 0,
            width: 1920,
            height: 1080,
        },
        work_area: PhysicalRect {
            x: 4000,
            y: 0,
            width: 1920,
            height: 1080,
        },
        scale: 1.0,
        primary: false,
    };
    let setup = MonitorSetup::new(vec![gone]).expect("gone monitor setup");
    let saved = Placement {
        setup: setup.fingerprint(),
        bounds: MonitorLocalLogicalRect {
            x: 100.,
            y: 80.,
            width: 1000.,
            height: 700.,
        },
        monitor: gone.bounds,
        maximized: false,
    };
    assert_eq!(opening_saved(&office(), &[saved]), None);
}

#[test]
fn first_place_on_1280x720_gives_1040x624_centred_on_navo() {
    let screen = Screen {
        id: DisplayId::new(1),
        bounds: logical(0., 0., 1280., 720.),
        work_area: logical(0., 0., 1280., 720.),
        scale: 1.0,
        primary: true,
    };
    let navo = logical(0., 0., 1280., 720.);
    let WindowBounds::Windowed(placed) = first_place(navo, &screen) else {
        panic!("expected a normal window, got a maximized or fullscreen one");
    };
    assert_eq!(placed.origin, point(px(120.), px(48.)));
    assert_eq!(placed.size, size(px(1040.), px(624.)));
}

#[test]
fn fit_on_clamps_oversized_150_percent_record_inside_100_percent_work_area() {
    let area = logical(0., 0., 1280., 720.);
    let oversized = WindowBounds::Windowed(logical(-100., -50., 1920., 1080.));
    let WindowBounds::Windowed(fitted) = fit_on(oversized, area) else {
        panic!("expected a normal window, got a maximized or fullscreen one");
    };
    assert_eq!(fitted.size, size(px(1280.), px(720.)));
    assert_eq!(fitted.origin, point(px(0.), px(0.)));
}
