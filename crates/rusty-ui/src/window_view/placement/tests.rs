use gpui_kit::{DisplayId, point};
use rusty_core::placement::{MonitorLocalLogicalRect, PhysicalRect};

use super::{Screen, Shown, opening, placement};
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
