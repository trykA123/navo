//! A small window for the probe tests. It opens one visible window and exits on `WM_CLOSE`
//! with code `key-downs + 100 × letter characters`.
//! Switches: `--late` waits 300 ms before the window, `--stubborn` ignores `WM_CLOSE`,
//! `--no-window` exits after 5 s without a window. Other arguments are ignored.

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |switch: &str| args.iter().any(|arg| arg == switch);
    if has("--no-window") {
        std::thread::sleep(std::time::Duration::from_secs(5));
        return;
    }
    if has("--late") {
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    window::STUBBORN.store(has("--stubborn"), std::sync::atomic::Ordering::Relaxed);
    std::process::exit(window::run());
}

#[cfg(windows)]
#[allow(unsafe_code, reason = "a raw Win32 window for the probe tests")]
mod window {
    use std::ptr::{null, null_mut};
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
        GetMessageW, MSG, PostQuitMessage, RegisterClassW, SW_SHOW, ShowWindow, TranslateMessage,
        WM_CHAR, WM_CLOSE, WM_DESTROY, WM_KEYDOWN, WNDCLASSW, WS_OVERLAPPEDWINDOW,
    };

    pub static STUBBORN: AtomicBool = AtomicBool::new(false);
    static KEY_DOWNS: AtomicI32 = AtomicI32::new(0);
    static LETTERS: AtomicI32 = AtomicI32::new(0);

    unsafe extern "system" fn proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_KEYDOWN => {
                KEY_DOWNS.fetch_add(1, Ordering::Relaxed);
            }
            WM_CHAR if char::from_u32(wparam as u32).is_some_and(|c| c.is_ascii_alphanumeric()) => {
                LETTERS.fetch_add(1, Ordering::Relaxed);
            }
            WM_CLOSE if STUBBORN.load(Ordering::Relaxed) => return 0,
            WM_CLOSE => {
                // SAFETY: `hwnd` is this window, destroyed on its own thread.
                unsafe { DestroyWindow(hwnd) };
                return 0;
            }
            WM_DESTROY => {
                let code =
                    KEY_DOWNS.load(Ordering::Relaxed) + 100 * LETTERS.load(Ordering::Relaxed);
                // SAFETY: ends this thread's message loop with `code`.
                unsafe { PostQuitMessage(code) };
                return 0;
            }
            _ => {}
        }
        // SAFETY: default handling for a message delivered to this window.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    pub fn run() -> i32 {
        let class: Vec<u16> = "NavoBenchTestee\0".encode_utf16().collect();
        // SAFETY: null asks for this executable's module handle.
        let instance = unsafe { GetModuleHandleW(null()) };
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..WNDCLASSW::default()
        };
        // SAFETY: `wc` and the class name outlive the call.
        if unsafe { RegisterClassW(&raw const wc) } == 0 {
            return -1;
        }
        // SAFETY: the class is registered above; every pointer is valid or null.
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                320,
                200,
                null_mut(),
                null_mut(),
                instance,
                null(),
            )
        };
        if hwnd.is_null() {
            return -2;
        }
        // SAFETY: `hwnd` was just created on this thread.
        unsafe { ShowWindow(hwnd, SW_SHOW) };
        let mut msg = MSG::default();
        // SAFETY: `msg` is a live output; the loop ends on WM_QUIT (0) or error (-1).
        while unsafe { GetMessageW(&raw mut msg, null_mut(), 0, 0) } > 0 {
            // SAFETY: `msg` was filled by GetMessageW.
            unsafe {
                TranslateMessage(&raw const msg);
                DispatchMessageW(&raw const msg);
            }
        }
        msg.wParam as i32
    }
}
