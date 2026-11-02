use super::*;

#[gpui_kit::test]
fn startup_pressure_cleans_over_quota_before_the_first_wait(cx: &mut gpui_kit::TestAppContext) {
    let profile = Profile::new();
    profile.delete_size("home/large.txt", 2 * 1024 * 1024);
    std::fs::write(
        profile.dirs.settings_file(),
        "[retention]\nmax-volume-mib = 1\n",
    )
    .unwrap();
    let mut schedule = schedule(super::super::super::super::now_ms());
    schedule.memory = Some(profile.journal.clone());
    let executor = cx.executor();
    let started = executor.now();
    super::super::run_with_pause(
        (&profile.dirs, profile.fs.as_ref()),
        (&mut schedule, &CancelToken::new()),
        |delay| {
            assert_eq!(
                profile.live(),
                0,
                "urgent cleanup must precede the startup delay"
            );
            assert_eq!(executor.now(), started);
            assert_eq!(delay, Duration::from_secs(60));
            false
        },
    );
}

#[gpui_kit::test]
fn startup_pressure_cleans_low_space_but_ordinary_expiry_waits(cx: &mut gpui_kit::TestAppContext) {
    for pressure in [false, true] {
        let profile = Profile::new();
        profile.delete("home/old.txt");
        if pressure {
            profile.fs.set_available_space(".navo-vault", Some(0));
        }
        let mut schedule = schedule(super::super::super::super::now_ms() + 4 * DAY_MS);
        schedule.memory = Some(profile.journal.clone());
        let executor = cx.executor();
        let started = executor.now();
        let mut waits = 0;
        super::super::run_with_pause(
            (&profile.dirs, profile.fs.as_ref()),
            (&mut schedule, &CancelToken::new()),
            |delay| {
                waits += 1;
                if waits > 1 {
                    return false;
                }
                assert_eq!(profile.live(), usize::from(!pressure));
                assert_eq!(executor.now(), started);
                executor.advance_clock(delay);
                true
            },
        );
        assert_eq!(profile.live(), 0);
        assert_eq!(executor.now() - started, Duration::from_secs(60));
    }
}
