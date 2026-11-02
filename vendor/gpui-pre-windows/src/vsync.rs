use std::{
    sync::LazyLock,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use gpui::FrameRequestSource;
use gpui_util::ResultExt;
use windows::Win32::{
    Foundation::HWND,
    Graphics::Dwm::{DWM_TIMING_INFO, DwmFlush, DwmGetCompositionTimingInfo},
    System::Performance::QueryPerformanceFrequency,
    UI::WindowsAndMessaging::{SWP_SHOWWINDOW, WM_SHOWWINDOW, WM_WINDOWPOSCHANGED},
};

use crate::FrameStart;

static QPC_TICKS_PER_SECOND: LazyLock<u64> = LazyLock::new(|| {
    let mut frequency = 0;
    // On systems that run Windows XP or later, the function will always succeed and
    // will thus never return zero.
    unsafe { QueryPerformanceFrequency(&mut frequency).unwrap() };
    frequency as u64
});

const VSYNC_INTERVAL_THRESHOLD: Duration = Duration::from_millis(1);
const DEFAULT_VSYNC_INTERVAL: Duration = Duration::from_micros(16_666); // ~60Hz
const SLOW_VSYNC_WAIT: Duration = Duration::from_millis(100);

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

    pub(crate) fn wait(&self, start: FrameStart) -> FrameRequestSource {
        let wait_start = Instant::now();
        let source = if start.after_park() {
            std::thread::sleep(self.interval);
            FrameRequestSource::LocalSchedule
        } else {
            self.wait_for_vsync()
        };
        let waited = wait_start.elapsed();
        match start {
            FrameStart::Signaled => {
                log::debug!("VSync frame resumed after a signaled park, waited {waited:?}")
            }
            FrameStart::Running if waited >= SLOW_VSYNC_WAIT => {
                log::debug!("VSync wait blocked for {waited:?}")
            }
            _ => log::trace!("VSync wait returned after {waited:?} ({start:?})"),
        }
        source
    }

    fn wait_for_vsync(&self) -> FrameRequestSource {
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

pub(crate) fn defers_vsync_wake(msg: u32, wparam: usize, flags: u32) -> bool {
    match msg {
        WM_SHOWWINDOW => wparam != 0,
        WM_WINDOWPOSCHANGED => flags & SWP_SHOWWINDOW.0 != 0,
        _ => false,
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
    use windows::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE};

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

    #[test]
    fn the_first_frame_after_a_park_never_waits_on_the_compositor() {
        let provider = VSyncProvider {
            interval: Duration::ZERO,
            f: Box::new(|| panic!("the compositor wait ran after a park")),
        };

        assert_eq!(
            provider.wait(FrameStart::Signaled),
            FrameRequestSource::LocalSchedule
        );
        assert_eq!(
            provider.wait(FrameStart::TimedOut),
            FrameRequestSource::LocalSchedule
        );
    }

    #[test]
    fn show_wakes_are_deferred_and_other_window_messages_are_not() {
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
    }
}
