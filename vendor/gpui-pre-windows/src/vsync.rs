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
    UI::WindowsAndMessaging::{
        SIZE_MINIMIZED, SWP_SHOWWINDOW, WM_ACTIVATE, WM_SHOWWINDOW, WM_SIZE, WM_WINDOWPOSCHANGED,
    },
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

pub(crate) fn wakes_vsync(msg: u32, wparam: usize, flags: u32) -> bool {
    match msg {
        WM_SHOWWINDOW => wparam != 0,
        WM_WINDOWPOSCHANGED => flags & SWP_SHOWWINDOW.0 != 0,
        WM_SIZE => wparam != SIZE_MINIMIZED as usize,
        WM_ACTIVATE => true,
        _ => false,
    }
}

pub(crate) static VSYNC_WAKE: VSyncWake = VSyncWake::new();

pub(crate) struct VSyncWake {
    pending: Mutex<bool>,
    condvar: Condvar,
    #[cfg(test)]
    parked: Mutex<Option<std::sync::mpsc::Sender<()>>>,
}

impl VSyncWake {
    const fn new() -> Self {
        Self {
            pending: Mutex::new(false),
            condvar: Condvar::new(),
            #[cfg(test)]
            parked: Mutex::new(None),
        }
    }

    pub(crate) fn signal(&self) {
        *self.pending.lock() = true;
        self.condvar.notify_all();
    }

    fn park(&self, stop: &AtomicBool) {
        let mut pending = self.pending.lock();
        while !*pending && !stop.load(Ordering::Acquire) {
            #[cfg(test)]
            if let Some(parked) = self.parked.lock().as_ref() {
                parked.send(()).unwrap();
            }
            self.condvar.wait(&mut pending);
        }
        *pending = false;
    }
}

pub(crate) fn run_vsync_loop(
    wake: &VSyncWake,
    stop: &AtomicBool,
    mut wanted: impl FnMut() -> bool,
    mut frame: impl FnMut() -> ControlFlow<()>,
) {
    loop {
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
    use std::sync::{Arc, mpsc};
    use windows::Win32::UI::WindowsAndMessaging::{
        SIZE_MAXIMIZED, SIZE_RESTORED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WA_ACTIVE,
        WA_CLICKACTIVE, WA_INACTIVE,
    };

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
    fn window_messages_wake_vsync_when_needed() {
        assert!(wakes_vsync(WM_SHOWWINDOW, 1, 0));
        assert!(!wakes_vsync(WM_SHOWWINDOW, 0, 0));
        let flags = (SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOMOVE).0;
        assert!(wakes_vsync(
            WM_WINDOWPOSCHANGED,
            0,
            flags | SWP_SHOWWINDOW.0
        ));
        assert!(!wakes_vsync(WM_WINDOWPOSCHANGED, 0, flags));
        assert!(wakes_vsync(WM_SIZE, SIZE_RESTORED as usize, 0));
        assert!(wakes_vsync(WM_SIZE, SIZE_MAXIMIZED as usize, 0));
        assert!(!wakes_vsync(WM_SIZE, SIZE_MINIMIZED as usize, 0));
        for activation in [WA_ACTIVE, WA_CLICKACTIVE, WA_INACTIVE] {
            assert!(wakes_vsync(WM_ACTIVATE, activation as usize, 0));
        }
        assert!(!wakes_vsync(0, 0, 0));
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
            let thread = std::thread::spawn({
                let (wake, stop, windows) = (wake.clone(), stop.clone(), windows.clone());
                move || {
                    run_vsync_loop(
                        &wake,
                        &stop,
                        || vsync_wanted(windows.lock().iter().copied()),
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
                thread,
            }
        }

        fn wait_for_park(&self) {
            self.parked.recv_timeout(BOUND).expect("loop did not park");
            drop(
                self.wake
                    .pending
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
        let flags = SWP_SHOWWINDOW | SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOMOVE;
        test.windows.lock()[0].visible = true;
        if wakes_vsync(WM_WINDOWPOSCHANGED, 0, flags.0) {
            test.wake.signal();
        }
        test.wait_for_frame();

        test.windows.lock()[0].visible = false;
        test.resume.send(()).unwrap();
        test.wait_for_park();
        test.stop_while_parked();
    }

    #[test]
    fn stop_while_parked_ends_the_loop_within_the_bound() {
        let test = TestLoop::new();
        test.wait_for_park();
        test.stop_while_parked();
    }
}
