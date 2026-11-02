use std::time::Duration;

/// Longest wait for the summoning window to come back to the front.
pub const FOCUS_WAIT: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PasteError {
    #[error("the window to paste into is gone")]
    Gone,
    #[error("the window to paste into did not take focus")]
    Refused,
    #[error("the system did not accept the paste keystrokes")]
    Blocked,
    #[error("pasting into another app is not supported on this platform")]
    Unsupported,
}

/// The window that was in front when Navo was summoned, kept as plain numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Foreground {
    #[cfg(windows)]
    window: usize,
    #[cfg(windows)]
    thread: u32,
    #[cfg(not(windows))]
    _unsupported: (),
}

#[cfg(not(windows))]
impl Foreground {
    #[must_use]
    pub fn now() -> Self {
        Self { _unsupported: () }
    }

    /// Always `Unsupported`: paste back needs Windows.
    pub fn paste(&self) -> Result<(), PasteError> {
        Err(PasteError::Unsupported)
    }

    /// Always false off Windows.
    #[must_use]
    pub fn is_own(&self) -> bool {
        false
    }
}

#[cfg(windows)]
mod platform {
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
        VIRTUAL_KEY, VK_CONTROL, VK_V,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, IsWindow, SetForegroundWindow,
    };

    use super::{FOCUS_WAIT, Foreground, PasteError};

    const POLL: Duration = Duration::from_millis(10);

    impl Foreground {
        /// Remembers the current foreground window; call it before Navo takes focus.
        #[must_use]
        pub fn now() -> Self {
            // SAFETY: GetForegroundWindow has no preconditions.
            let window = unsafe { GetForegroundWindow() };
            Self {
                window: window as usize,
                thread: thread_of(window),
            }
        }

        /// Brings the window back to the front and sends Ctrl+V to it.
        ///
        /// Blocks for up to [`FOCUS_WAIT`]: call it from a worker thread, never the UI thread.
        pub fn paste(&self) -> Result<(), PasteError> {
            self.check()?;
            if !self.is_front() {
                // SAFETY: the handle passed IsWindow; a refusal shows up in the wait below.
                unsafe { SetForegroundWindow(self.handle()) };
            }
            self.wait_front()?;
            send_ctrl_v()
        }

        /// True when the window belongs to this process, such as Navo's own window.
        #[must_use]
        pub fn is_own(&self) -> bool {
            let mut process = 0_u32;
            // SAFETY: the out pointer is valid; a stale handle returns 0 and leaves it unset.
            let thread = unsafe { GetWindowThreadProcessId(self.handle(), &mut process) };
            thread != 0 && process == std::process::id()
        }

        fn handle(&self) -> HWND {
            self.window as HWND
        }

        fn check(&self) -> Result<(), PasteError> {
            // SAFETY: IsWindow accepts any handle value.
            let alive = self.window != 0 && unsafe { IsWindow(self.handle()) } != 0;
            if alive && thread_of(self.handle()) == self.thread {
                Ok(())
            } else {
                Err(PasteError::Gone)
            }
        }

        fn is_front(&self) -> bool {
            // SAFETY: GetForegroundWindow has no preconditions.
            let front = unsafe { GetForegroundWindow() };
            front as usize == self.window
        }

        fn wait_front(&self) -> Result<(), PasteError> {
            let deadline = Instant::now() + FOCUS_WAIT;
            loop {
                self.check()?;
                match (self.is_front(), Instant::now() >= deadline) {
                    (true, _) => return Ok(()),
                    (false, true) => return Err(PasteError::Refused),
                    (false, false) => std::thread::sleep(POLL),
                }
            }
        }
    }

    fn thread_of(window: HWND) -> u32 {
        // SAFETY: a null process-id pointer is allowed; a stale handle returns 0.
        unsafe { GetWindowThreadProcessId(window, std::ptr::null_mut()) }
    }

    fn send_ctrl_v() -> Result<(), PasteError> {
        let keys = [
            key(VK_CONTROL, 0),
            key(VK_V, 0),
            key(VK_V, KEYEVENTF_KEYUP),
            key(VK_CONTROL, KEYEVENTF_KEYUP),
        ];
        if send(&keys) == keys.len() {
            return Ok(());
        }
        // Key-ups for keys that never went down are harmless; a stuck Ctrl is not.
        send(&keys[2..]);
        Err(PasteError::Blocked)
    }

    fn key(code: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: code,
                    dwFlags: flags,
                    ..KEYBDINPUT::default()
                },
            },
        }
    }

    fn send(inputs: &[INPUT]) -> usize {
        let count = u32::try_from(inputs.len()).unwrap_or(0);
        let size = i32::try_from(size_of::<INPUT>()).unwrap_or(0);
        // SAFETY: inputs holds `count` initialised keyboard INPUTs of `size` bytes each.
        let sent = unsafe { SendInput(count, inputs.as_ptr(), size) };
        usize::try_from(sent).unwrap_or(0)
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::{Foreground, PasteError};

    #[test]
    fn paste_is_unsupported_off_windows() {
        let target = Foreground::now();
        assert_eq!(target.paste(), Err(PasteError::Unsupported));
        assert!(!target.is_own());
        assert_eq!(target, Foreground::now());
    }
}
