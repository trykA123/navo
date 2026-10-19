use super::*;

fn arbitrary_monitor() -> impl Strategy<Value = Monitor> {
    (
        -20000..20000,
        -20000..20000,
        1..8000_u32,
        1..5000_u32,
        0..6_usize,
    )
        .prop_map(|(x, y, width, height, scale)| {
            monitor(
                physical(x, y, width, height),
                [0.75, 1.0, 1.25, 1.5, 2.0, 3.0][scale],
            )
        })
}

fn assert_contained(current: &MonitorSetup, result: Restored) -> Result<(), TestCaseError> {
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
    fn exact_setups_keep_valid_saved_bounds_inside_the_saved_monitor(
        monitors in prop::collection::vec(arbitrary_monitor(), 1..5),
        index in any::<usize>(),
        maximized in any::<bool>(),
    ) {
        let current = setup(monitors);
        let index = index % current.monitors().len();
        let monitor = current.monitors()[index];
        let mut saved = placement(&current, index);
        saved.bounds = logical(0.0, 0.0,
            f64::from(monitor.work_area.width) / f64::from(monitor.scale),
            f64::from(monitor.work_area.height) / f64::from(monitor.scale));
        saved.maximized = maximized;
        let result = restore(&current, &[saved.clone()]);
        prop_assert_eq!(result.bounds, saved.bounds);
        prop_assert_eq!(result.maximized, maximized);
        assert_contained(&current, result)?;
    }
}
