use std::cell::RefCell;
use std::ptr;
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;

use windows_sys::Win32::Foundation::{
    CloseHandle, HANDLE, HWND, LPARAM, LRESULT, WAIT_FAILED, WAIT_OBJECT_0, WPARAM,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{CreateEventW, INFINITE, SetEvent};
use windows_sys::Win32::UI::Shell::{
    SHCNE_DRIVEADD, SHCNE_DRIVEADDGUI, SHCNE_DRIVEREMOVED, SHCNE_MEDIAINSERTED, SHCNE_MEDIAREMOVED,
    SHCNE_NETSHARE, SHCNE_NETUNSHARE, SHCNRF_InterruptLevel, SHCNRF_NewDelivery, SHCNRF_ShellLevel,
    SHChangeNotification_Lock, SHChangeNotification_Unlock, SHChangeNotifyDeregister,
    SHChangeNotifyEntry, SHChangeNotifyRegister,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DEV_BROADCAST_HDR, DefWindowProcW, DestroyWindow, DispatchMessageW, MSG,
    MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW, PostQuitMessage,
    QS_ALLINPUT, RegisterClassW, TranslateMessage, WM_APP, WM_CLOSE, WM_DEVICECHANGE, WM_QUIT,
    WNDCLASSW, WS_EX_TOOLWINDOW,
};

use super::{device_event, reads_device_header};

const SHELL_CHANGE: u32 = WM_APP + 1;
const MESSAGES_PER_WAKE: usize = 64;
const DRIVE_EVENTS: u32 = SHCNE_DRIVEADD
    | SHCNE_DRIVEREMOVED
    | SHCNE_MEDIAINSERTED
    | SHCNE_MEDIAREMOVED
    | SHCNE_NETSHARE
    | SHCNE_NETUNSHARE
    | SHCNE_DRIVEADDGUI;

type OnChange = Box<dyn Fn()>;

thread_local! {
    static ON_CHANGE: RefCell<Option<OnChange>> = const { RefCell::new(None) };
}

struct StopEvent(usize);

impl StopEvent {
    fn create() -> Option<Self> {
        // SAFETY: no security attributes or name; creates an unnamed, manual-reset event.
        let event = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
        if event.is_null() {
            tracing::warn!(error = %std::io::Error::last_os_error(), "creating volume stop event failed");
            return None;
        }
        Some(Self(event as usize))
    }

    fn handle(&self) -> HANDLE {
        self.0 as HANDLE
    }

    fn signal(&self) {
        // SAFETY: this event remains open while the watcher and thread hold their Arcs.
        if unsafe { SetEvent(self.handle()) } == 0 {
            tracing::warn!(error = %std::io::Error::last_os_error(), "stopping volume watcher failed");
        }
    }
}

impl Drop for StopEvent {
    fn drop(&mut self) {
        // SAFETY: the last owner closes the event exactly once, after the waiter exits.
        unsafe { CloseHandle(self.handle()) };
    }
}

pub struct VolumeWatch {
    stop: Arc<StopEvent>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for VolumeWatch {
    fn drop(&mut self) {
        self.stop.signal();
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::warn!("volume watcher panicked");
        }
    }
}

fn notify() {
    super::super::windows::invalidate_remembered();
    ON_CHANGE.with_borrow(|on_change| {
        if let Some(call) = on_change {
            call();
        }
    });
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn shell_changed(wparam: WPARAM, lparam: LPARAM) {
    let Ok(process) = u32::try_from(lparam) else {
        return;
    };
    let mut items = ptr::null_mut();
    let mut event = 0;
    // SAFETY: NewDelivery supplies the shared notification handle and sender process ID.
    let lock = unsafe {
        SHChangeNotification_Lock(wparam as HANDLE, process, &raw mut items, &raw mut event)
    };
    if lock.is_null() {
        tracing::warn!("locking shell drive notification failed");
        return;
    }
    // SAFETY: this lock came from SHChangeNotification_Lock; no item pointers escape.
    if unsafe { SHChangeNotification_Unlock(lock) } == 0 {
        tracing::warn!("unlocking shell drive notification failed");
    }
    notify();
}

fn device_changed(wparam: WPARAM, lparam: LPARAM) {
    let device_type = if reads_device_header(wparam) && lparam != 0 {
        // SAFETY: only arrival/removal events supply a non-null broadcast header.
        Some(unsafe { (*(lparam as *const DEV_BROADCAST_HDR)).dbch_devicetype })
    } else {
        None
    };
    if device_event(wparam, device_type).is_some() {
        notify();
    }
}

unsafe extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_DEVICECHANGE => {
            device_changed(wparam, lparam);
            1
        }
        SHELL_CHANGE => {
            shell_changed(wparam, lparam);
            0
        }
        WM_CLOSE => {
            // SAFETY: queues quit for the owning thread, which deregisters before destruction.
            unsafe { PostQuitMessage(0) };
            0
        }
        // SAFETY: unhandled messages retain their original window and arguments.
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

fn create() -> Option<HWND> {
    let class = wide("NavoVolumeWatch");
    // SAFETY: a null name asks for the module of the running executable.
    let instance = unsafe { GetModuleHandleW(ptr::null()) };
    let registration = WNDCLASSW {
        lpfnWndProc: Some(procedure),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        ..WNDCLASSW::default()
    };
    // SAFETY: `registration` points at a NUL-terminated class name that outlives the call;
    // a second registration fails harmlessly and the class from the first one is reused.
    unsafe { RegisterClassW(&raw const registration) };
    // SAFETY: the class is registered, both strings are NUL-terminated, and the window has
    // no WS_VISIBLE, so it is never shown.
    let window = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            instance,
            ptr::null(),
        )
    };
    (!window.is_null()).then_some(window)
}

fn register(window: HWND) -> u32 {
    let entry = SHChangeNotifyEntry {
        pidl: ptr::null_mut(),
        fRecursive: 1,
    };
    // SAFETY: window belongs to this thread; a null PIDL watches the entire shell namespace.
    let registration = unsafe {
        SHChangeNotifyRegister(
            window,
            SHCNRF_ShellLevel | SHCNRF_InterruptLevel | SHCNRF_NewDelivery,
            DRIVE_EVENTS as i32,
            SHELL_CHANGE,
            1,
            &raw const entry,
        )
    };
    if registration == 0 {
        tracing::warn!(error = %std::io::Error::last_os_error(), "registering shell drive notifications failed");
    }
    registration
}

fn drain_messages() -> bool {
    let mut message = MSG::default();
    for _ in 0..MESSAGES_PER_WAKE {
        // SAFETY: the live MSG receives messages belonging to this thread only.
        if unsafe { PeekMessageW(&raw mut message, ptr::null_mut(), 0, 0, PM_REMOVE) } == 0 {
            break;
        }
        if message.message == WM_QUIT {
            return false;
        }
        // SAFETY: PeekMessageW filled this MSG; dispatch runs on the window's owning thread.
        unsafe {
            TranslateMessage(&raw const message);
            DispatchMessageW(&raw const message);
        }
    }
    true
}

fn pump(stop: &StopEvent) {
    let event = stop.handle();
    loop {
        // SAFETY: the event stays open for this call; the queue belongs to this thread.
        let result = unsafe {
            MsgWaitForMultipleObjectsEx(
                1,
                &raw const event,
                INFINITE,
                QS_ALLINPUT,
                MWMO_INPUTAVAILABLE,
            )
        };
        let pending = match result {
            WAIT_OBJECT_0 => false,
            result if result == WAIT_OBJECT_0 + 1 => drain_messages(),
            WAIT_FAILED => {
                tracing::warn!(error = %std::io::Error::last_os_error(), "waiting for volume messages failed");
                false
            }
            result => {
                tracing::warn!(result, "unexpected volume message wait result");
                false
            }
        };
        if !pending {
            return;
        }
    }
}

#[must_use]
pub fn watch_volumes(on_change: impl Fn() + Send + 'static) -> Option<VolumeWatch> {
    let stop = Arc::new(StopEvent::create()?);
    let thread_stop = stop.clone();
    let (sender, receiver) = mpsc::sync_channel(1);
    let thread = std::thread::Builder::new()
        .name("volume-watch".to_owned())
        .spawn(move || {
            ON_CHANGE.set(Some(Box::new(on_change)));
            let Some(window) = create() else {
                tracing::warn!(error = %std::io::Error::last_os_error(), "creating volume window failed");
                return;
            };
            let registration = register(window);
            if sender.send(registration != 0).is_ok() && registration != 0 {
                pump(&thread_stop);
            }
            // SAFETY: this thread owns the nonzero registration, and its window is still alive.
            if registration != 0 && unsafe { SHChangeNotifyDeregister(registration) } == 0 {
                tracing::warn!("deregistering shell drive notifications failed");
            }
            // SAFETY: only this owning thread destroys its hidden window, after deregistration.
            if unsafe { DestroyWindow(window) } == 0 {
                tracing::warn!(error = %std::io::Error::last_os_error(), "destroying volume window failed");
            }
            ON_CHANGE.set(None);
        })
        .inspect_err(|error| tracing::warn!(%error, "starting volume watcher failed"))
        .ok()?;
    let watch = VolumeWatch {
        stop,
        thread: Some(thread),
    };
    receiver.recv().unwrap_or(false).then_some(watch)
}
