use std::sync::{Weak, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::service::Updater;

const RECHECK: Duration = Duration::from_secs(24 * 60 * 60);
const RESUME_CHECK: Duration = Duration::from_secs(60);

pub(super) struct Schedule {
    due: Duration,
    pub(super) is_start: bool,
}

impl Schedule {
    pub(super) fn new(now: Duration, delay: Duration) -> Self {
        Self {
            due: now.saturating_add(delay),
            is_start: true,
        }
    }

    pub(super) fn take_due(&mut self, now: Duration) -> bool {
        if now < self.due {
            return false;
        }
        self.due = now.saturating_add(RECHECK);
        self.is_start = false;
        true
    }

    fn wait(&self, now: Duration) -> Duration {
        RESUME_CHECK.min(self.due.saturating_sub(now))
    }
}

pub(super) struct Timer {
    stop: mpsc::Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl Timer {
    pub(super) fn start(updater: Weak<Updater>, delay: Duration) -> std::io::Result<Self> {
        let (stop, receiver) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("navo-update-timer".into())
            .spawn(move || run(updater, delay, receiver))?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}

fn run(updater: Weak<Updater>, delay: Duration, receiver: mpsc::Receiver<()>) {
    let mut schedule = None;
    loop {
        let mut wait = RESUME_CHECK;
        if let Some(now) = now() {
            let Some(updater) = updater.upgrade() else {
                return;
            };
            let schedule = schedule.get_or_insert_with(|| Schedule::new(now, delay));
            updater.tick(schedule, now);
            wait = schedule.wait(now);
        }
        if receiver.recv_timeout(wait) != Err(mpsc::RecvTimeoutError::Timeout) {
            return;
        }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take()
            && thread.thread().id() != thread::current().id()
            && thread.join().is_err()
        {
            tracing::warn!("update: the timer stopped unexpectedly");
        }
    }
}

#[cfg(windows)]
fn now() -> Option<Duration> {
    // SAFETY: GetTickCount64 has no arguments and includes suspended time.
    Some(Duration::from_millis(unsafe {
        windows_sys::Win32::System::SystemInformation::GetTickCount64()
    }))
}

#[cfg(target_os = "linux")]
fn now() -> Option<Duration> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: time points to a writable timespec for the duration of the call.
    if unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &raw mut time) } != 0 {
        tracing::warn!(error = %std::io::Error::last_os_error(), "update: cannot read the timer clock");
        return None;
    }
    Some(Duration::new(
        u64::try_from(time.tv_sec).ok()?,
        u32::try_from(time.tv_nsec).ok()?,
    ))
}

#[cfg(not(any(windows, target_os = "linux")))]
fn now() -> Option<Duration> {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    Some(START.get_or_init(std::time::Instant::now).elapsed())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_tracks_the_due_time_and_checks_for_resume_within_a_minute() {
        let schedule = Schedule::new(Duration::ZERO, Duration::from_secs(90));
        for (now, wait) in [(0, 60), (30, 60), (31, 59), (89, 1), (90, 0), (120, 0)] {
            assert_eq!(
                schedule.wait(Duration::from_secs(now)),
                Duration::from_secs(wait)
            );
        }
    }
}
