use super::*;

fn arbitrary_monitor() -> impl Strategy<Value = Monitor> {
    (
        -20000..20000,
        -20000..20000,
        6..8000_u32,
        6..5000_u32,
        0..6_usize,
    )
        .prop_flat_map(|(x, y, width, height, scale)| {
            (
                Just(monitor(
                    physical(x, y, width, height),
                    [0.75, 1.0, 1.25, 1.5, 2.0, 3.0][scale],
                )),
                2..width / 2,
                2..width / 2,
                2..height / 2,
                2..height / 2,
            )
        })
        .prop_map(|(mut monitor, left, right, top, bottom)| {
            monitor.work_area = physical(
                monitor.bounds.x + i32::try_from(left).unwrap(),
                monitor.bounds.y + i32::try_from(top).unwrap(),
                monitor.bounds.width - left - right,
                monitor.bounds.height - top - bottom,
            );
            monitor
        })
}

pub(super) fn assert_contained(
    current: &MonitorSetup,
    result: Restored,
) -> Result<(), TestCaseError> {
    let monitor = current
        .monitors()
        .iter()
        .find(|m| m.bounds == result.monitor)
        .unwrap();
    let scale = f64::from(monitor.scale);
    let bounds = result.bounds;
    let x = f64::from(monitor.bounds.x) + bounds.x * scale;
    let y = f64::from(monitor.bounds.y) + bounds.y * scale;
    let area = monitor.work_area;
    prop_assert!(bounds.width > 0.0 && bounds.height > 0.0);
    prop_assert!(x >= f64::from(area.x) - 1e-8);
    prop_assert!(y >= f64::from(area.y) - 1e-8);
    prop_assert!(x + bounds.width * scale <= f64::from(area.x) + f64::from(area.width) + 1e-8);
    prop_assert!(y + bounds.height * scale <= f64::from(area.y) + f64::from(area.height) + 1e-8);
    Ok(())
}

proptest! {
    #[test]
    fn changed_setups_always_restore_positive_bounds_inside_a_current_work_area(
        monitors in prop::collection::vec(arbitrary_monitor(), 1..5),
        old_monitor in arbitrary_monitor(),
        x in -20000.0..20000.0_f64,
        y in -20000.0..20000.0_f64,
        width in 0.001..40000.0_f64,
        height in 0.001..40000.0_f64,
        maximized in any::<bool>(),
        has_placement in any::<bool>(),
    ) {
        let current = setup(monitors);
        let old = setup(vec![old_monitor]);
        prop_assume!(current.fingerprint() != old.fingerprint());
        let mut saved = placement(&old, 0);
        saved.bounds = logical(x, y, width, height);
        saved.maximized = maximized;
        let placements = if has_placement { vec![saved] } else { Vec::new() };
        assert_contained(&current, restore(&current, &placements))?;
    }

    #[test]
    fn exact_setups_restore_positive_bounds_inside_changed_work_areas(
        monitors in prop::collection::vec(arbitrary_monitor(), 1..5),
        index in any::<usize>(),
        x in -20000.0..20000.0_f64,
        y in -20000.0..20000.0_f64,
        width in 0.001..40000.0_f64,
        height in 0.001..40000.0_f64,
        maximized in any::<bool>(),
    ) {
        let mut old_monitors = monitors.clone();
        for monitor in &mut old_monitors {
            monitor.work_area.x -= 1;
            monitor.work_area.width += 1;
        }
        let old = setup(old_monitors);
        let current = setup(monitors);
        let index = index % current.monitors().len();
        let monitor = current.monitors()[index];
        prop_assert_eq!(old.fingerprint(), current.fingerprint());
        prop_assert_ne!(old.monitors()[index].work_area, monitor.work_area);
        let mut saved = placement(&old, index);
        saved.bounds = logical(x, y, width, height);
        saved.maximized = maximized;
        let result = restore(&current, &[saved]);
        prop_assert_eq!(result.monitor, monitor.bounds);
        prop_assert_eq!(result.maximized, maximized);
        assert_contained(&current, result)?;
    }
}
