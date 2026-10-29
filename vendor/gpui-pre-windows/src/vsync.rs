use std::{
    ops::ControlFlow,
    sync::{
        LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use gpui::FrameRequestSource;
use gpui_util::ResultExt;
use parking_lot::{Condvar, Mutex};
use windows::Win32::{
    Foundation::HWND,
    Graphics::Dwm::{DWM_TIMING_INFO, DwmFlush, DwmGetCompositionTimingInfo},
    System::Performance::QueryPerformanceFrequency,
    UI::WindowsAndMessaging::{SWP_SHOWWINDOW, WM_SHOWWINDOW, WM_WINDOWPOSCHANGED},
};

static QPC_TICKS_PER_SECOND: LazyLock<u64> = LazyLock::new(|| {
    let mut frequency = 0;
    // On systems that run Windows XP or later, the function will always succeed and
    // will thus never return zero.
    unsafe { QueryPerformanceFrequency(&mut frequency).unwrap() };
    frequency as u64
});

const VSYNC_INTERVAL_THRESHOLD: Duration = Duration::from_millis(1);
const DEFAULT_VSYNC_INTERVAL: Duration = Duration::from_micros(16_666); // ~60Hz

pub(crate) struct VSyncProvider {
    interval: Duration,
    f: Box<dyn Fn() -> bool>,
}

impl VSyncProvider {
    pub(crate) fn new() -> Self {
        let interval = get_dwm_interval()
            .context("Failed to get DWM interval")
            .log_err()
            .unwrap_or(DEFAULT_VSYNC_INTERVAL);
        let f = Box::new(|| unsafe { DwmFlush().is_ok() });
        Self { interval, f }
    }

    pub(crate) fn wait_for_vsync(&self) -> FrameRequestSource {
        let vsync_start = Instant::now();
        let wait_succeeded = (self.f)();
        let elapsed = vsync_start.elapsed();
        // DwmFlush and DCompositionWaitForCompositorClock returns very early
        // instead of waiting until vblank when the monitor goes to sleep or is
        // unplugged (nothing to present due to desktop occlusion). We use 1ms as
        // a threshold for the duration of the wait functions and fallback to
        // Sleep() if it returns before that. This could happen during normal
        // operation for the first call after the vsync thread becomes non-idle,
        // but it shouldn't happen often.
        if !wait_succeeded || elapsed < VSYNC_INTERVAL_THRESHOLD {
            log::trace!("VSyncProvider::wait_for_vsync() took less time than expected");
            std::thread::sleep(self.interval);
            FrameRequestSource::LocalSchedule
        } else {
            FrameRequestSource::NativeCallback
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct WindowShow {
    pub(crate) visible: bool,
    pub(crate) minimized: bool,
}

pub(crate) fn vsync_wanted(mut windows: impl Iterator<Item = WindowShow>) -> bool {
    windows.any(|window| window.visible && !window.minimized)
}

pub(crate) fn defers_vsync_wake(msg: u32, wparam: usize, flags: u32) -> bool {
    match msg {
        WM_SHOWWINDOW => wparam != 0,
        WM_WINDOWPOSCHANGED => flags & SWP_SHOWWINDOW.0 != 0,
        _ => false,
    }
}

const IDLE_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Clone, Copy)]
struct IdleGate {
    last_present: Instant,
    last_activity: Instant,
}

impl IdleGate {
    fn new(now: Instant) -> Self {
        Self {
            last_present: now,
            last_activity: now,
        }
    }

    fn should_park(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.last_present) >= IDLE_TIMEOUT
            && now.saturating_duration_since(self.last_activity) >= IDLE_TIMEOUT
    }
}

fn should_park(has_visible_window: bool, gate: &IdleGate, now: Instant) -> bool {
    !has_visible_window || gate.should_park(now)
}

struct WakeState {
    pending: bool,
    gate: IdleGate,
}

pub(crate) static VSYNC_WAKE: LazyLock<VSyncWake> = LazyLock::new(VSyncWake::new);

pub(crate) struct VSyncWake {
    state: Mutex<WakeState>,
    condvar: Condvar,
    #[cfg(test)]
    parked: Mutex<Option<std::sync::mpsc::Sender<()>>>,
}

impl VSyncWake {
    fn new() -> Self {
        Self {
            state: Mutex::new(WakeState {
                pending: false,
                gate: IdleGate::new(Instant::now()),
            }),
            condvar: Condvar::new(),
            #[cfg(test)]
            parked: Mutex::new(None),
        }
    }

    pub(crate) fn signal(&self) {
        let mut state = self.state.lock();
        let now = Instant::now();
        state.gate.last_activity = now;
        state.pending = true;
        self.condvar.notify_all();
    }

    pub(crate) fn presented(&self) {
        let mut state = self.state.lock();
        let now = Instant::now();
        state.gate.last_present = now;
        state.pending = true;
        self.condvar.notify_all();
    }

    pub(crate) fn activity(&self) -> VSyncActivity<'_> {
        self.signal();
        VSyncActivity(self)
    }

    pub(crate) fn window_message(&self, polled_paint: bool) -> Option<VSyncActivity<'_>> {
        if polled_paint {
            None
        } else {
            Some(self.activity())
        }
    }

    fn consume_signal(&self) {
        self.state.lock().pending = false;
    }

    pub(crate) fn should_park(&self, has_visible_window: bool, now: Instant) -> bool {
        should_park(has_visible_window, &self.state.lock().gate, now)
    }

    fn park(&self, stop: &AtomicBool) {
        let mut state = self.state.lock();
        while !state.pending && !stop.load(Ordering::Acquire) {
            #[cfg(test)]
            if let Some(parked) = self.parked.lock().as_ref() {
                parked.send(()).unwrap();
            }
            self.condvar.wait(&mut state);
        }
        state.pending = false;
    }
}

pub(crate) struct VSyncActivity<'a>(&'a VSyncWake);

impl Drop for VSyncActivity<'_> {
    fn drop(&mut self) {
        self.0.signal();
    }
}

pub(crate) fn run_vsync_loop(
    wake: &VSyncWake,
    stop: &AtomicBool,
    mut wanted: impl FnMut() -> bool,
    mut frame: impl FnMut() -> ControlFlow<()>,
) {
    loop {
        wake.consume_signal();
        if !wanted() {
            wake.park(stop);
        }
        if stop.load(Ordering::Acquire) || frame().is_break() {
            break;
        }
    }
}

fn get_dwm_interval() -> Result<Duration> {
    let mut timing_info = DWM_TIMING_INFO {
        cbSize: std::mem::size_of::<DWM_TIMING_INFO>() as u32,
        ..Default::default()
    };
    unsafe { DwmGetCompositionTimingInfo(HWND::default(), &mut timing_info) }?;
    let interval = retrieve_duration(timing_info.qpcRefreshPeriod, *QPC_TICKS_PER_SECOND);
    // Check for interval values that are impossibly low. A 29 microsecond
    // interval was seen (from a qpcRefreshPeriod of 60).
    if interval < VSYNC_INTERVAL_THRESHOLD {
        Ok(retrieve_duration(
            timing_info.rateRefresh.uiDenominator as u64,
            timing_info.rateRefresh.uiNumerator as u64,
        ))
    } else {
        Ok(interval)
    }
}

#[inline]
fn retrieve_duration(counts: u64, ticks_per_second: u64) -> Duration {
    let ticks_per_microsecond = ticks_per_second / 1_000_000;
    Duration::from_micros(counts / ticks_per_microsecond)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        rc::Rc,
        sync::{Arc, mpsc},
    };
    use windows::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE};

    const BOUND: Duration = Duration::from_secs(2);

    #[test]
    fn successful_compositor_wait_reports_native_callback() {
        let provider = VSyncProvider {
            interval: Duration::ZERO,
            f: Box::new(|| {
                std::thread::sleep(VSYNC_INTERVAL_THRESHOLD * 2);
                true
            }),
        };

        assert_eq!(
            provider.wait_for_vsync(),
            FrameRequestSource::NativeCallback
        );
    }

    #[test]
    fn failed_compositor_wait_reports_local_schedule_even_after_threshold() {
        let provider = VSyncProvider {
            interval: Duration::ZERO,
            f: Box::new(|| {
                std::thread::sleep(VSYNC_INTERVAL_THRESHOLD * 2);
                false
            }),
        };

        assert_eq!(provider.wait_for_vsync(), FrameRequestSource::LocalSchedule);
    }

    fn show(visible: bool, minimized: bool) -> WindowShow {
        WindowShow { visible, minimized }
    }

    #[test]
    fn vsync_is_not_wanted_while_every_window_is_hidden() {
        assert!(!vsync_wanted(std::iter::empty()));
        assert!(!vsync_wanted(
            [show(false, false), show(false, true)].into_iter()
        ));
    }

    #[test]
    fn vsync_is_wanted_while_one_window_is_visible_and_not_minimized() {
        assert!(vsync_wanted(
            [show(false, false), show(true, false)].into_iter()
        ));
    }

    #[test]
    fn vsync_is_not_wanted_for_a_visible_minimized_window() {
        assert!(!vsync_wanted([show(true, true)].into_iter()));
    }

    #[test]
    fn fresh_present_runs() {
        let start = Instant::now();
        let gate = IdleGate {
            last_present: start + Duration::from_millis(550),
            ..IdleGate::new(start)
        };
        assert!(!gate.should_park(start + Duration::from_millis(600)));
    }

    #[test]
    fn idle_after_600ms_parks() {
        let start = Instant::now();
        assert!(IdleGate::new(start).should_park(start + Duration::from_millis(600)));
    }

    #[test]
    fn both_deadlines_must_expire() {
        let start = Instant::now();
        let mut gate = IdleGate::new(start);
        assert!(!gate.should_park(start + IDLE_TIMEOUT - Duration::from_nanos(1)));
        assert!(gate.should_park(start + IDLE_TIMEOUT));
        gate.last_activity = start + IDLE_TIMEOUT;
        assert!(!gate.should_park(start + IDLE_TIMEOUT));
        assert!(!gate.should_park(start));
    }

    #[test]
    fn activity_after_park_runs() {
        let start = Instant::now();
        let mut gate = IdleGate::new(start);
        let now = start + Duration::from_millis(600);
        assert!(gate.should_park(now));
        gate.last_activity = now;
        assert!(!gate.should_park(now));
    }

    #[test]
    fn animations_run_hidden_windows_park() {
        let start = Instant::now();
        let mut gate = IdleGate::new(start);
        for tick in 1..100 {
            gate.last_present = start + Duration::from_millis(tick * 16);
            assert!(!should_park(true, &gate, gate.last_present));
            assert!(should_park(false, &gate, gate.last_present));
        }
    }

    #[test]
    fn a_signal_before_parking_is_not_lost() {
        let wake = Arc::new(VSyncWake::new());
        wake.signal();
        let (done_tx, done_rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            wake.park(&AtomicBool::new(false));
            done_tx.send(()).unwrap();
        });
        done_rx.recv_timeout(BOUND).expect("the signal was lost");
        thread.join().unwrap();
    }

    struct TestLoop {
        wake: Arc<VSyncWake>,
        stop: Arc<AtomicBool>,
        windows: Arc<Mutex<Vec<WindowShow>>>,
        parked: mpsc::Receiver<()>,
        frames: mpsc::Receiver<()>,
        resume: mpsc::Sender<()>,
        done: mpsc::Receiver<()>,
        now: Instant,
        thread: std::thread::JoinHandle<()>,
    }

    impl TestLoop {
        fn new() -> Self {
            let wake = Arc::new(VSyncWake::new());
            let stop = Arc::new(AtomicBool::new(false));
            let windows = Arc::new(Mutex::new(Vec::<WindowShow>::new()));
            let (parked_tx, parked_rx) = mpsc::channel();
            *wake.parked.lock() = Some(parked_tx);
            let (frame_tx, frame_rx) = mpsc::channel();
            let (resume_tx, resume_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            let now = Instant::now();
            let thread = std::thread::spawn({
                let (wake, stop, windows) = (wake.clone(), stop.clone(), windows.clone());
                move || {
                    run_vsync_loop(
                        &wake,
                        &stop,
                        || !wake.should_park(vsync_wanted(windows.lock().iter().copied()), now),
                        || {
                            frame_tx.send(()).unwrap();
                            resume_rx.recv_timeout(BOUND).expect("frame did not finish");
                            ControlFlow::Continue(())
                        },
                    );
                    done_tx.send(()).unwrap();
                }
            });
            Self {
                wake,
                stop,
                windows,
                parked: parked_rx,
                frames: frame_rx,
                resume: resume_tx,
                done: done_rx,
                now,
                thread,
            }
        }

        fn wait_for_park(&self) {
            self.parked.recv_timeout(BOUND).expect("loop did not park");
            drop(
                self.wake
                    .state
                    .try_lock_for(BOUND)
                    .expect("loop did not release the park mutex"),
            );
            assert_eq!(self.frames.try_recv(), Err(mpsc::TryRecvError::Empty));
        }

        fn wait_for_frame(&self) {
            self.frames
                .recv_timeout(BOUND)
                .expect("loop did not resume");
        }

        fn stop_while_parked(self) {
            self.stop.store(true, Ordering::Release);
            self.wake.signal();
            self.done.recv_timeout(BOUND).expect("loop did not stop");
            self.thread.join().unwrap();
            assert!(self.frames.try_recv().is_err());
        }
    }

    #[test]
    fn registration_after_parking_wakes_the_loop() {
        let test = TestLoop::new();
        test.wait_for_park();
        test.windows.lock().push(show(true, false));
        test.wake.signal();
        test.wait_for_frame();

        test.windows.lock()[0].visible = false;
        test.resume.send(()).unwrap();
        test.wait_for_park();
        test.stop_while_parked();
    }

    #[test]
    fn windowpos_show_after_parking_wakes_the_loop() {
        let test = TestLoop::new();
        test.wait_for_park();
        test.windows.lock().push(show(false, false));
        drop(test.wake.window_message(false));
        test.wait_for_frame();
        test.resume.send(()).unwrap();
        test.wait_for_park();

        let flags = SWP_NOACTIVATE.0 | SWP_NOSIZE.0 | SWP_NOMOVE.0;
        assert!(!defers_vsync_wake(WM_WINDOWPOSCHANGED, 0, flags));
        assert!(!defers_vsync_wake(WM_SHOWWINDOW, 0, 0));
        assert!(defers_vsync_wake(WM_SHOWWINDOW, 1, 0));
        assert!(!defers_vsync_wake(0, 0, 0));
        assert!(defers_vsync_wake(
            WM_WINDOWPOSCHANGED,
            0,
            flags | SWP_SHOWWINDOW.0
        ));
        test.windows.lock()[0].visible = true;
        if defers_vsync_wake(WM_WINDOWPOSCHANGED, 0, flags | SWP_SHOWWINDOW.0) {
            test.wake.signal();
        }
        test.wait_for_frame();
        test.windows.lock()[0].visible = false;
        test.resume.send(()).unwrap();
        test.wait_for_park();
        test.stop_while_parked();
    }

    #[test]
    fn visible_idle_dispatch_wakes() {
        let test = TestLoop::new();
        test.wait_for_park();
        test.windows.lock().push(show(true, false));
        test.wake.signal();
        test.wait_for_frame();
        expire_gate(&test.wake, test.now);
        test.resume.send(()).unwrap();
        test.wait_for_park();

        let activity = test.wake.activity();
        test.wait_for_frame();
        expire_gate(&test.wake, test.now);
        drop(activity);
        test.resume.send(()).unwrap();
        test.wait_for_frame();
        expire_gate(&test.wake, test.now);
        test.resume.send(()).unwrap();
        test.wait_for_park();
        test.stop_while_parked();
    }

    #[test]
    fn poll_does_not_extend_deadline() {
        let wake = VSyncWake::new();
        expire_gate(&wake, Instant::now());
        let gate = wake.state.lock().gate;
        drop(wake.window_message(true));
        assert_eq!(wake.state.lock().gate.last_activity, gate.last_activity);
        assert!(wake.should_park(true, Instant::now()));
        assert!(!wake.state.lock().pending);
        drop(wake.window_message(false));
        let last_activity = wake.state.lock().gate.last_activity;
        assert!(!wake.should_park(true, last_activity));
        assert!(wake.state.lock().pending);
    }

    #[test]
    fn polled_paint_does_not_wake() {
        let test = TestLoop::new();
        test.wait_for_park();
        test.windows.lock().push(show(true, false));
        test.wake.signal();
        test.wait_for_frame();
        expire_gate(&test.wake, test.now);
        test.resume.send(()).unwrap();
        test.wait_for_park();

        let state = test
            .wake
            .state
            .try_lock_for(BOUND)
            .expect("loop did not release the park mutex");
        assert!(test.wake.window_message(true).is_none());
        assert_eq!(
            test.wake.condvar.notify_all(),
            1,
            "a polled paint must leave the waiter parked"
        );
        drop(state);
        test.wait_for_park();
        test.stop_while_parked();
    }

    #[test]
    fn frame_waker_wakes_parked_loop() {
        let test = TestLoop::new();
        test.wait_for_park();
        test.windows.lock().push(show(true, false));
        test.wake.signal();
        test.wait_for_frame();
        expire_gate(&test.wake, test.now);
        test.resume.send(()).unwrap();
        test.wait_for_park();

        let frame_waker: Rc<dyn Fn()> = Rc::new({
            let wake = test.wake.clone();
            move || wake.signal()
        });
        frame_waker();
        test.wait_for_frame();
        expire_gate(&test.wake, test.now);
        test.resume.send(()).unwrap();
        test.wait_for_park();
        test.stop_while_parked();
    }

    #[test]
    fn present_wakes() {
        let test = TestLoop::new();
        test.wait_for_park();
        test.windows.lock().push(show(true, false));
        test.wake.presented();
        test.wait_for_frame();
        expire_gate(&test.wake, test.now);
        test.resume.send(()).unwrap();
        test.wait_for_park();
        test.stop_while_parked();
    }

    #[test]
    fn activity_before_park_is_retained() {
        let wake = Arc::new(VSyncWake::new());
        let stop = Arc::new(AtomicBool::new(false));
        let (checked_tx, checked_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let (frame_tx, frame_rx) = mpsc::channel();
        let thread = std::thread::spawn({
            let (wake, stop) = (wake.clone(), stop.clone());
            move || {
                run_vsync_loop(
                    &wake,
                    &stop,
                    || {
                        checked_tx.send(()).unwrap();
                        resume_rx
                            .recv_timeout(BOUND)
                            .expect("visibility check did not resume");
                        false
                    },
                    || {
                        frame_tx.send(()).unwrap();
                        ControlFlow::Break(())
                    },
                );
            }
        });
        checked_rx
            .recv_timeout(BOUND)
            .expect("visibility was not checked");
        drop(wake.window_message(false));
        resume_tx.send(()).unwrap();
        frame_rx
            .recv_timeout(BOUND)
            .expect("activity before parking was lost");
        thread.join().unwrap();
    }

    #[test]
    fn registration_before_park_is_retained() {
        let wake = Arc::new(VSyncWake::new());
        let windows = Arc::new(Mutex::new(Vec::<WindowShow>::new()));
        let (checked_tx, checked_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let (frame_tx, frame_rx) = mpsc::channel();
        let thread = std::thread::spawn({
            let (wake, windows) = (wake.clone(), windows.clone());
            move || {
                run_vsync_loop(
                    &wake,
                    &AtomicBool::new(false),
                    || {
                        let visible = vsync_wanted(windows.lock().iter().copied());
                        checked_tx.send(()).unwrap();
                        resume_rx
                            .recv_timeout(BOUND)
                            .expect("registration did not finish");
                        !wake.should_park(visible, Instant::now())
                    },
                    || {
                        frame_tx.send(()).unwrap();
                        ControlFlow::Break(())
                    },
                );
            }
        });
        checked_rx
            .recv_timeout(BOUND)
            .expect("visibility was not checked");
        windows.lock().push(show(true, false));
        wake.signal();
        resume_tx.send(()).unwrap();
        frame_rx
            .recv_timeout(BOUND)
            .expect("registration wake was lost");
        thread.join().unwrap();
    }

    fn expire_gate(wake: &VSyncWake, now: Instant) {
        wake.state.lock().gate = IdleGate::new(now - Duration::from_millis(600));
    }

    #[test]
    fn stop_while_parked_ends_the_loop_within_the_bound() {
        let test = TestLoop::new();
        test.wait_for_park();
        test.stop_while_parked();
    }
}
