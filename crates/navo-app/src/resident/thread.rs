use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW,
    TranslateMessage, WM_HOTKEY, WM_QUIT, WM_USER,
};

use super::events::{ResidentConfig, Sink};
use super::keys::{Input, event_for};
use super::{hotkey, tray};

const START_WAIT: Duration = Duration::from_secs(2);
const STOP_WAIT: Duration = Duration::from_secs(2);

pub(crate) struct Resident {
    running: Option<Running>,
    taken: Arc<AtomicBool>,
    tray: Arc<AtomicBool>,
}

struct Running {
    thread_id: u32,
    thread: JoinHandle<()>,
    done: mpsc::Receiver<()>,
}

impl Resident {
    pub(crate) fn start(config: ResidentConfig, sink: Sink) -> Self {
        let taken = Arc::new(AtomicBool::new(false));
        let tray = Arc::new(AtomicBool::new(false));
        let running = if config.is_idle() {
            None
        } else {
            spawn(config, sink, Flags(taken.clone(), tray.clone()))
        };
        Self {
            running,
            taken,
            tray,
        }
    }

    pub(crate) fn has_tray(&self) -> bool {
        self.tray.load(Ordering::Acquire)
    }

    pub(crate) fn key_taken(&self) -> bool {
        self.taken.load(Ordering::Acquire)
    }

    #[cfg(test)]
    pub(super) fn thread_id(&self) -> Option<u32> {
        self.running.as_ref().map(|running| running.thread_id)
    }

    pub(super) fn stop(&mut self) -> bool {
        let Some(running) = self.running.take() else {
            return true;
        };
        // SAFETY: posting to a thread id passes no pointers; the thread created its queue before reporting the id.
        if unsafe { PostThreadMessageW(running.thread_id, WM_QUIT, 0, 0) } == 0 {
            let error = std::io::Error::last_os_error();
            tracing::error!(%error, "cannot stop the resident thread; detaching it");
            return false;
        }
        match running.done.recv_timeout(STOP_WAIT) {
            Err(RecvTimeoutError::Timeout) => {
                tracing::error!("the resident thread did not stop; detaching it");
                false
            }
            Ok(()) | Err(RecvTimeoutError::Disconnected) => running.thread.join().is_ok(),
        }
    }
}

impl Drop for Resident {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Flags(Arc<AtomicBool>, Arc<AtomicBool>);

fn spawn(config: ResidentConfig, sink: Sink, flags: Flags) -> Option<Running> {
    let (ready, ready_rx) = mpsc::sync_channel(1);
    let (done, done_rx) = mpsc::channel::<()>();
    let thread = thread::Builder::new()
        .name("navo-resident".into())
        .spawn(move || {
            let _done = done;
            run(config, &sink, &flags, &ready);
        })
        .inspect_err(|error| tracing::warn!(%error, "cannot start the resident thread"))
        .ok()?;
    match ready_rx.recv_timeout(START_WAIT) {
        Ok(thread_id) => Some(Running {
            thread_id,
            thread,
            done: done_rx,
        }),
        Err(error) => {
            tracing::error!(%error, "the resident thread did not start; detaching it");
            None
        }
    }
}

fn run(config: ResidentConfig, sink: &Sink, flags: &Flags, ready: &mpsc::SyncSender<u32>) {
    let mut message = MSG::default();
    // SAFETY: `message` is a live MSG; PM_NOREMOVE only creates this thread's message queue.
    unsafe {
        PeekMessageW(
            &raw mut message,
            std::ptr::null_mut(),
            WM_USER,
            WM_USER,
            PM_NOREMOVE,
        )
    };
    let hotkey = config.summon_key.and_then(hotkey::Hotkey::register);
    flags.0.store(
        config.summon_key.is_some() && hotkey.is_none(),
        Ordering::Release,
    );
    let icon = if config.keep_in_tray {
        tray::create()
    } else {
        None
    };
    flags.1.store(icon.is_some(), Ordering::Release);
    tray::drain(|_| {});
    // SAFETY: GetCurrentThreadId has no preconditions.
    if ready.send(unsafe { GetCurrentThreadId() }).is_err() {
        return;
    }
    let deliver = |input: Input<'_>| {
        if let Some(event) = event_for(input, Instant::now()) {
            sink(event);
        }
    };
    loop {
        // SAFETY: `message` is a live, writable MSG; a null window reads every message of this thread.
        let got = unsafe { GetMessageW(&raw mut message, std::ptr::null_mut(), 0, 0) };
        if got <= 0 {
            break;
        }
        if message.message == WM_HOTKEY {
            deliver(Input::Hotkey(i32::try_from(message.wParam).unwrap_or(-1)));
        } else {
            // SAFETY: `message` was just filled by GetMessageW on this thread.
            unsafe {
                TranslateMessage(&raw const message);
                DispatchMessageW(&raw const message);
            }
        }
        tray::drain(deliver);
    }
    drop(icon);
    drop(hotkey);
}

#[cfg(test)]
#[path = "thread_tests.rs"]
mod tests;
