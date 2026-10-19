use proptest::prelude::*;

use super::*;
use crate::{AppState, Location, SessionState};

mod props;
mod session;

fn physical(x: i32, y: i32, width: u32, height: u32) -> PhysicalRect {
    PhysicalRect {
        x,
        y,
        width,
        height,
    }
}

fn logical(x: f64, y: f64, width: f64, height: f64) -> MonitorLocalLogicalRect {
    MonitorLocalLogicalRect {
        x,
        y,
        width,
        height,
    }
}

fn monitor(bounds: PhysicalRect, scale: f32) -> Monitor {
    Monitor {
        bounds,
        work_area: bounds,
        scale,
        primary: false,
    }
}

fn setup(monitors: Vec<Monitor>) -> MonitorSetup {
    MonitorSetup::new(monitors).unwrap()
}

fn single() -> MonitorSetup {
    setup(vec![monitor(physical(0, 0, 1920, 1080), 1.0)])
}

fn placement(setup: &MonitorSetup, index: usize) -> Placement {
    Placement {
        setup: setup.fingerprint(),
        bounds: logical(100.0, 80.0, 1000.0, 700.0),
        monitor: setup.monitors()[index].bounds,
        maximized: false,
    }
}

#[test]
fn fingerprint_sorts_bounds_and_scales_but_excludes_work_areas() {
    let first = monitor(physical(-2560, 0, 2560, 1440), 1.25);
    let mut second = monitor(physical(0, 0, 1920, 1080), 1.0);
    let original = setup(vec![first, second]);
    second.work_area.height -= 40;
    assert_eq!(
        original.fingerprint(),
        setup(vec![second, first]).fingerprint()
    );
    second.scale = 1.5;
    assert_ne!(
        original.fingerprint(),
        setup(vec![second, first]).fingerprint()
    );
}

#[test]
fn exact_match_beats_a_newer_placement_and_ignores_display_enumeration_order() {
    let left = monitor(physical(-1920, 0, 1920, 1080), 1.0);
    let right = monitor(physical(0, 0, 2560, 1440), 1.5);
    let saved = setup(vec![left, right]);
    let expected = placement(&saved, 0);
    let current = setup(vec![right, left]);
    let result = restore(&current, &[placement(&single(), 0), expected.clone()]);
    assert_eq!(result.bounds, expected.bounds);
    assert_eq!(result.monitor, left.bounds);
    assert!(!result.maximized);
}

#[test]
fn exact_match_keeps_maximized_and_normal_bounds_on_the_saved_monitor() {
    let current = setup(vec![
        monitor(physical(0, 0, 1920, 1080), 1.0),
        monitor(physical(1920, 0, 2560, 1440), 1.5),
    ]);
    let mut saved = placement(&current, 1);
    saved.maximized = true;
    let result = restore(&current, &[saved.clone()]);
    assert_eq!(result.bounds, saved.bounds);
    assert_eq!(result.monitor, saved.monitor);
    assert!(result.maximized);
}

#[test]
fn exact_match_moves_bounds_inside_after_the_taskbar_moves_to_the_top() {
    let mut original = monitor(physical(0, 0, 1920, 1080), 1.0);
    original.work_area.height -= 40;
    let old = setup(vec![original]);
    let mut saved = placement(&old, 0);
    saved.bounds = logical(100.0, 0.0, 1000.0, 1040.0);
    let mut current = original;
    current.work_area.y = 40;
    let current = setup(vec![current]);
    assert_eq!(old.fingerprint(), current.fingerprint());
    let result = restore(&current, &[saved]);
    assert_eq!(result.bounds, logical(100.0, 40.0, 1000.0, 1040.0));
    props::assert_contained(&current, result).unwrap();
}

#[test]
fn exact_match_keeps_bounds_already_inside_the_changed_work_area() {
    let old = single();
    let saved = placement(&old, 0);
    let mut current = old.monitors()[0];
    current.work_area = physical(0, 40, 1920, 1040);
    let result = restore(&setup(vec![current]), std::slice::from_ref(&saved));
    assert_eq!(result.bounds, saved.bounds);
}

#[test]
fn exact_match_shrinks_bounds_to_fit_the_changed_work_area() {
    let old = single();
    let mut saved = placement(&old, 0);
    saved.bounds = logical(-100.0, 0.0, 2100.0, 1080.0);
    let mut current = old.monitors()[0];
    current.work_area = physical(0, 40, 1920, 1040);
    let result = restore(&setup(vec![current]), &[saved]);
    assert_eq!(result.bounds, logical(0.0, 40.0, 1920.0, 1040.0));
}

#[test]
fn moved_monitor_clamps_a_window_with_exactly_a_quarter_visible() {
    let mut saved = placement(&single(), 0);
    saved.bounds = logical(1200.0, 100.0, 800.0, 600.0);
    let current = setup(vec![monitor(physical(1800, 0, 1920, 1080), 1.0)]);
    let result = restore(&current, &[saved]);
    assert_eq!(result.bounds, logical(0.0, 100.0, 800.0, 600.0));
    assert_eq!(result.monitor, current.monitors()[0].bounds);
}

#[test]
fn smaller_monitor_shrinks_the_newest_window_to_its_work_area() {
    let mut saved = placement(&single(), 0);
    saved.bounds = logical(100.0, 100.0, 1400.0, 900.0);
    let mut smaller = monitor(physical(0, 0, 1024, 768), 1.0);
    smaller.work_area = physical(0, 24, 1024, 704);
    let result = restore(&setup(vec![smaller]), &[saved]);
    assert_eq!(result.bounds, logical(0.0, 24.0, 1024.0, 704.0));
}

#[test]
fn a_gone_monitor_centres_on_primary_and_drops_maximized() {
    let old = setup(vec![monitor(physical(4000, 0, 1920, 1080), 1.0)]);
    let mut saved = placement(&old, 0);
    saved.maximized = true;
    let result = restore(&single(), &[saved]);
    assert_eq!(result.bounds, logical(320.0, 140.0, 1280.0, 800.0));
    assert!(!result.maximized);
}

#[test]
fn no_placements_uses_eighty_percent_and_the_monitor_at_origin() {
    let mut primary = monitor(physical(0, 0, 1000, 750), 1.0);
    primary.work_area = physical(0, 50, 1000, 700);
    let current = setup(vec![monitor(physical(-1920, 0, 1920, 1080), 1.0), primary]);
    let result = restore(&current, &[]);
    assert_eq!(result.bounds, logical(100.0, 120.0, 800.0, 560.0));
    assert_eq!(result.monitor, primary.bounds);
}

#[test]
fn no_placements_uses_the_flagged_primary_before_the_origin_monitor() {
    let mut primary = monitor(physical(1920, 0, 1920, 1080), 1.0);
    primary.primary = true;
    let current = setup(vec![single().monitors()[0], primary]);
    let result = restore(&current, &[]);
    assert_eq!(result.bounds, logical(320.0, 140.0, 1280.0, 800.0));
    assert_eq!(result.monitor, primary.bounds);
}

#[test]
fn no_monitor_at_origin_uses_the_first_monitor() {
    let current = setup(vec![
        monitor(physical(2000, -500, 1920, 1080), 1.0),
        monitor(physical(-1920, 0, 1920, 1080), 1.0),
    ]);
    let result = restore(&current, &[]);
    assert_eq!(result.bounds, logical(320.0, 140.0, 1280.0, 800.0));
    assert_eq!(result.monitor, current.monitors()[0].bounds);
}

#[test]
fn mixed_scales_compare_physical_overlap_and_keep_logical_window_size() {
    let old = setup(vec![monitor(physical(1920, 0, 2560, 1440), 2.0)]);
    let mut saved = placement(&old, 0);
    saved.bounds = logical(100.0, 50.0, 800.0, 600.0);
    let mut target = monitor(physical(1920, 0, 1920, 1080), 1.5);
    target.work_area = physical(1920, 30, 1920, 1020);
    let current = setup(vec![monitor(physical(0, 0, 1920, 1080), 1.0), target]);
    let result = restore(&current, &[saved]);
    assert_eq!(
        result.bounds,
        logical(200.0 / 1.5, 100.0 / 1.5, 800.0, 600.0)
    );
    assert_eq!(result.monitor, target.bounds);
}

#[test]
fn fallback_does_not_search_older_nonmatching_placements() {
    let gone = setup(vec![monitor(physical(8000, 0, 1920, 1080), 1.0)]);
    let older = setup(vec![monitor(physical(0, 0, 2000, 1080), 1.0)]);
    let result = restore(&single(), &[placement(&gone, 0), placement(&older, 0)]);
    assert_eq!(result.bounds, logical(320.0, 140.0, 1280.0, 800.0));
}

#[test]
fn invalid_placement_metadata_falls_back_without_discarding_the_session() {
    let mut saved = placement(&single(), 0);
    saved.bounds.width = f64::NAN;
    let result = restore(&single(), &[saved]);
    assert_eq!(result.bounds, logical(320.0, 140.0, 1280.0, 800.0));
}

#[test]
fn empty_or_invalid_monitor_setups_are_rejected() {
    assert!(MonitorSetup::new(Vec::new()).is_err());
    let valid = monitor(physical(0, 0, 1920, 1080), 1.0);
    for scale in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(MonitorSetup::new(vec![Monitor { scale, ..valid }]).is_err());
    }
    for work_area in [physical(0, 0, 0, 1), physical(-1, 0, 1920, 1080)] {
        assert!(MonitorSetup::new(vec![Monitor { work_area, ..valid }]).is_err());
    }
}
