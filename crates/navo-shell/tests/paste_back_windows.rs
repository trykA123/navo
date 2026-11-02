#![cfg(windows)]

use std::ptr::{null, null_mut};
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use navo_shell::{Foreground, PasteError};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_V};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, DispatchMessageW, GetForegroundWindow,
    GetWindowThreadProcessId, MSG, PM_REMOVE, PeekMessageW, SetForegroundWindow, TranslateMessage,
    WM_KEYDOWN, WM_SYSKEYDOWN, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

const WAIT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(10);

static FRONT: Mutex<()> = Mutex::new(());

struct Window(HWND);

impl Window {
    fn new() -> Self {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // SAFETY: STATIC is a system class and the visible window belongs to this thread.
        let window = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                100,
                100,
                240,
                120,
                null_mut(),
                null_mut(),
                null_mut(),
                null(),
            )
        };
        assert!(!window.is_null());
        Self(window)
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: this thread created the window.
        unsafe { DestroyWindow(self.0) };
    }
}

/// Pumps this thread's messages; true when `target` saw V go down while Ctrl was held.
fn pump(target: HWND) -> bool {
    let mut pasted = false;
    let mut message = MSG::default();
    // SAFETY: message is a live, writable MSG; a null window reads every message of this thread.
    while unsafe { PeekMessageW(&raw mut message, null_mut(), 0, 0, PM_REMOVE) } != 0 {
        let down = matches!(message.message, WM_KEYDOWN | WM_SYSKEYDOWN);
        // SAFETY: GetKeyState reads this thread's key state as of the message just removed.
        let control = unsafe { GetKeyState(i32::from(VK_CONTROL)) } < 0;
        pasted |= down && message.hwnd == target && message.wParam == usize::from(VK_V) && control;
        // SAFETY: message was just filled by PeekMessageW on this thread.
        unsafe {
            TranslateMessage(&raw const message);
            DispatchMessageW(&raw const message);
        }
    }
    pasted
}

fn foreground() -> HWND {
    // SAFETY: GetForegroundWindow has no preconditions.
    unsafe { GetForegroundWindow() }
}

/// Test setup only: borrows the foreground thread's input state so this process may take focus.
fn bring_to_front(window: HWND) {
    let deadline = Instant::now() + WAIT;
    while foreground() != window {
        assert!(
            Instant::now() < deadline,
            "the test window never came to the front"
        );
        // SAFETY: GetCurrentThreadId has no preconditions; a null process-id pointer is allowed.
        let (own, front) = unsafe {
            (
                GetCurrentThreadId(),
                GetWindowThreadProcessId(foreground(), null_mut()),
            )
        };
        // SAFETY: both ids name live threads or AttachThreadInput fails harmlessly.
        let attached =
            front != 0 && front != own && unsafe { AttachThreadInput(own, front, 1) } != 0;
        // SAFETY: window is a live window created on this thread.
        unsafe { SetForegroundWindow(window) };
        if attached {
            // SAFETY: this thread attached to `front` just above.
            unsafe { AttachThreadInput(own, front, 0) };
        }
        pump(window);
        thread::sleep(POLL);
    }
}

#[test]
#[ignore = "needs an interactive desktop; run by the desktop tester"]
fn paste_returns_focus_and_sends_ctrl_v() {
    let _front = FRONT.lock();
    let target = Window::new();
    let other = Window::new();
    bring_to_front(target.0);
    let summoned = Foreground::now();
    bring_to_front(other.0);
    let (sender, results) = mpsc::channel();
    let worker = thread::spawn(move || {
        let _ = sender.send(summoned.paste());
    });
    let deadline = Instant::now() + WAIT;
    let mut result = None;
    let mut pasted = false;
    while Instant::now() < deadline && !(pasted && result.is_some()) {
        pasted |= pump(target.0);
        result = result.or_else(|| results.try_recv().ok());
        thread::sleep(POLL);
    }
    assert!(worker.join().is_ok());
    assert_eq!(result, Some(Ok(())));
    assert_eq!(foreground(), target.0);
    assert!(pasted);
}

#[test]
#[ignore = "needs an interactive desktop; run by the desktop tester"]
fn a_destroyed_window_is_gone() {
    let _front = FRONT.lock();
    let window = Window::new();
    bring_to_front(window.0);
    let summoned = Foreground::now();
    drop(window);
    assert_eq!(summoned.paste(), Err(PasteError::Gone));
}
