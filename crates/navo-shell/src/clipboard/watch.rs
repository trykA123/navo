use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::thread::JoinHandle;
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_CLASS_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::System::DataExchange::{
    AddClipboardFormatListener, GetClipboardOwner, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, RegisterClipboardFormatW, RemoveClipboardFormatListener,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowThreadProcessId, HWND_MESSAGE, KillTimer, MSG, PostMessageW, RegisterClassExW,
    SetTimer, WM_APP, WM_CLIPBOARDUPDATE, WM_TIMER, WNDCLASSEXW,
};

use super::capture::{admits_history, exe_stem, text_from_bytes};
use super::platform::{Clipboard, read_data, read_open};
use super::{CaptureContent, ClipboardError, Effect, ShellCapture};

type OnChange = Box<dyn Fn(ShellCapture) + Send>;

const CHANGED: u32 = WM_APP + 1;
const STOP: u32 = WM_APP + 2;
pub(super) const BUSY_ATTEMPTS: u32 = 5;
pub(super) const BUSY_DELAY: Duration = Duration::from_millis(20);
/// The first wait before a busy change is read again; each further retry doubles it.
const RETRY_FIRST: Duration = Duration::from_millis(50);
/// The longest wait between retries of a busy change.
const RETRY_MAX: Duration = Duration::from_secs(2);
/// The watcher window's timer id for retrying a busy change.
const RETRY_TIMER: usize = 1;
const START_WAIT: Duration = Duration::from_secs(5);
const STOP_WAIT: Duration = Duration::from_secs(2);
const IMAGE_UNITS: usize = 32_768;
const PRIVATE_MARKERS: [&str; 2] = [
    "ExcludeClipboardContentFromMonitorProcessing",
    "Clipboard Viewer Ignore",
];

/// Calls `on_change` on its own thread for every clipboard change history may keep.
/// Dropping it stops the thread, waiting at most two seconds.
pub struct Watcher {
    window: usize,
    done: Receiver<()>,
    thread: Option<JoinHandle<()>>,
}

impl Watcher {
    pub fn start(on_change: OnChange) -> Result<Self, ClipboardError> {
        let (ready, started) = mpsc::sync_channel(1);
        let (finished, done) = mpsc::channel::<()>();
        let thread = std::thread::Builder::new()
            .name("navo-clipboard-watch".to_owned())
            .spawn(move || {
                let _finished = finished;
                run(&on_change, &ready);
            })
            .map_err(|error| {
                ClipboardError::Os(format!("Starting the clipboard watcher: {error}"))
            })?;
        match started.recv_timeout(START_WAIT) {
            Ok(Ok(window)) => Ok(Self {
                window,
                done,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                finish(&done, Some(thread));
                Err(error)
            }
            Err(_) => Err(ClipboardError::Os(
                "The clipboard watcher did not start in time".to_owned(),
            )),
        }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        if matches!(self.done.try_recv(), Err(TryRecvError::Empty)) {
            // SAFETY: posting takes no pointers; a window already gone only makes the call fail.
            let posted = unsafe { PostMessageW(self.window as HWND, STOP, 0, 0) } != 0;
            if !posted {
                let error = std::io::Error::last_os_error();
                tracing::debug!(%error, "clipboard watcher stop not posted");
            }
        }
        finish(&self.done, self.thread.take());
    }
}

fn finish(done: &Receiver<()>, thread: Option<JoinHandle<()>>) {
    match done.recv_timeout(STOP_WAIT) {
        Ok(()) | Err(RecvTimeoutError::Disconnected) => {
            if let Some(thread) = thread {
                let _ = thread.join();
            }
        }
        Err(RecvTimeoutError::Timeout) => tracing::debug!("clipboard watcher did not stop in time"),
    }
}

fn run(on_change: &OnChange, ready: &SyncSender<Result<usize, ClipboardError>>) {
    let window = match Window::new() {
        Ok(window) => window,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let _listener = match Listener::new(&window) {
        Ok(listener) => listener,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    if ready.send(Ok(window.0 as usize)).is_ok() {
        let mut reader = Reader {
            window: window.0,
            on_change,
            last: None,
            backoff: None,
        };
        reader.pump();
    }
}

/// Reads clipboard changes on the watcher thread. A change the clipboard is too busy to read
/// is kept and retried on a timer with a doubling wait (at most `RETRY_MAX`) until it reads, so
/// the message loop never sleeps on a busy clipboard.
struct Reader<'a> {
    window: HWND,
    on_change: &'a OnChange,
    last: Option<u32>,
    /// The wait before the pending retry, while a busy change waits to be read.
    backoff: Option<Duration>,
}

impl Reader<'_> {
    fn pump(&mut self) {
        let mut message = MSG::default();
        loop {
            // SAFETY: message outlives the call; a null window reads every message of this thread.
            let got = unsafe { GetMessageW(&mut message, ptr::null_mut(), 0, 0) };
            if got == 0 || got == -1 || message.message == STOP {
                return;
            }
            let retry = message.message == WM_TIMER && message.wParam == RETRY_TIMER;
            if retry || message.message == CHANGED || message.message == WM_CLIPBOARDUPDATE {
                self.read();
                continue;
            }
            // SAFETY: message came from GetMessageW on this thread.
            unsafe { DispatchMessageW(&message) };
        }
    }

    /// Reads the current change; a busy clipboard schedules a retry instead of dropping it.
    fn read(&mut self) {
        if changed(&mut self.last, self.on_change) {
            self.retry_later();
        } else if self.backoff.take().is_some() {
            // SAFETY: the window is live and owned by this thread; a missing timer only fails.
            unsafe { KillTimer(self.window, RETRY_TIMER) };
        }
    }

    fn retry_later(&mut self) {
        let wait = self
            .backoff
            .map_or(RETRY_FIRST, |wait| wait.saturating_mul(2).min(RETRY_MAX));
        self.backoff = Some(wait);
        let millis = u32::try_from(wait.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: the window is live and owned by this thread; the same id replaces a pending timer.
        if unsafe { SetTimer(self.window, RETRY_TIMER, millis, None) } == 0 {
            self.backoff = None;
            let error = std::io::Error::last_os_error();
            tracing::debug!(%error, "clipboard retry timer not set; change dropped");
        }
    }
}

/// Reads one change and hands it to `on_change`; returns true when the clipboard was busy and
/// the change must be read again.
fn changed(last: &mut Option<u32>, on_change: &OnChange) -> bool {
    let capture = match read_change(last) {
        Ok(Some(capture)) => capture,
        Ok(None) => return false,
        Err(ClipboardError::Busy) => {
            tracing::debug!("clipboard busy; change kept for a retry");
            return true;
        }
        Err(error) => {
            tracing::debug!(error = error_kind(&error), "clipboard change dropped");
            return false;
        }
    };
    tracing::debug!(
        kind = capture.content.kind(),
        len = capture.content.len(),
        "clipboard change"
    );
    if catch_unwind(AssertUnwindSafe(|| on_change(capture))).is_err() {
        tracing::debug!("clipboard change handler panicked");
    }
    false
}

fn read_change(last: &mut Option<u32>) -> Result<Option<ShellCapture>, ClipboardError> {
    let _clipboard = Clipboard::open_with(ptr::null_mut(), BUSY_ATTEMPTS, BUSY_DELAY)?;
    let sequence = sequence();
    if sequence != 0 && *last == Some(sequence) {
        return Ok(None);
    }
    *last = Some(sequence);
    if is_private()? {
        return Ok(None);
    }
    let content = if let Some(bytes) = read_data(u32::from(CF_UNICODETEXT))? {
        CaptureContent::Text(text_from_bytes(&bytes))
    } else if let Some(files) = read_open()? {
        CaptureContent::Files {
            paths: files.paths,
            cut: files.effect == Effect::Move,
        }
    } else {
        return Ok(None);
    };
    Ok(Some(ShellCapture {
        content,
        owner_exe: owner_exe(),
        sequence,
    }))
}

fn is_private() -> Result<bool, ClipboardError> {
    for name in PRIVATE_MARKERS {
        let format = register(name)?;
        // SAFETY: format is a registered identifier; the query takes no pointers.
        if unsafe { IsClipboardFormatAvailable(format) } != 0 {
            return Ok(true);
        }
    }
    let marker = read_data(register("CanIncludeInClipboardHistory")?)?;
    Ok(!admits_history(marker.as_deref()))
}

fn sequence() -> u32 {
    // SAFETY: the sequence query takes no pointers and does not render clipboard data.
    unsafe { GetClipboardSequenceNumber() }
}

pub(super) fn register(name: &str) -> Result<u32, ClipboardError> {
    let wide = wide(name);
    // SAFETY: wide is a live NUL-terminated UTF-16 string.
    let format = unsafe { RegisterClipboardFormatW(wide.as_ptr()) };
    if format == 0 {
        return Err(os_error("Registering a clipboard format"));
    }
    Ok(format)
}

pub(super) fn owner_exe() -> Option<String> {
    exe_stem(&owner_image()?)
}

/// The clipboard owner's full image path, best effort.
pub(super) fn owner_exe_path() -> Option<String> {
    Some(String::from_utf16_lossy(&owner_image()?))
}

fn owner_image() -> Option<Vec<u16>> {
    // SAFETY: the owner query takes no arguments; the clipboard is open, so the owner is stable.
    let owner = unsafe { GetClipboardOwner() };
    if owner.is_null() {
        return None;
    }
    let mut process_id = 0u32;
    // SAFETY: a stale owner handle only makes the call fail; process_id outlives the call.
    unsafe { GetWindowThreadProcessId(owner, &mut process_id) };
    let process = Process::open(process_id)?;
    let mut image = vec![0u16; IMAGE_UNITS];
    let mut units = IMAGE_UNITS as u32;
    // SAFETY: process is a live query handle; image holds units UTF-16 slots and units outlives the call.
    let named = unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            image.as_mut_ptr(),
            &mut units,
        )
    };
    if named == 0 {
        return None;
    }
    image.truncate(units as usize);
    Some(image)
}

struct Process(HANDLE);

impl Process {
    fn open(process_id: u32) -> Option<Self> {
        if process_id == 0 {
            return None;
        }
        // SAFETY: OpenProcess takes plain values and returns null on failure.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
        (!handle.is_null()).then_some(Self(handle))
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: the handle came from OpenProcess and is closed once here.
        unsafe { CloseHandle(self.0) };
    }
}

struct Window(HWND);

impl Window {
    fn new() -> Result<Self, ClipboardError> {
        let class = wide("NavoClipboardWatch");
        // SAFETY: a null name asks for the module of the running executable.
        let instance = unsafe { GetModuleHandleW(ptr::null()) };
        let info = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..WNDCLASSEXW::default()
        };
        // SAFETY: info and the class name it points to live for the call.
        let registered = unsafe { RegisterClassExW(&info) } != 0;
        // SAFETY: GetLastError reads this thread's last error and takes no pointers.
        if !registered && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS {
            return Err(os_error("Registering the clipboard watcher window"));
        }
        // SAFETY: the class is registered; HWND_MESSAGE makes an invisible window owned by this thread.
        let window = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                ptr::null_mut(),
                instance,
                ptr::null(),
            )
        };
        if window.is_null() {
            return Err(os_error("Creating the clipboard watcher window"));
        }
        Ok(Self(window))
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: this thread created the window and the listener guard has already been removed.
        unsafe { DestroyWindow(self.0) };
    }
}

struct Listener<'a>(&'a Window);

impl<'a> Listener<'a> {
    fn new(window: &'a Window) -> Result<Self, ClipboardError> {
        // SAFETY: window is live and owned by this thread.
        if unsafe { AddClipboardFormatListener(window.0) } == 0 {
            return Err(os_error("Listening for clipboard changes"));
        }
        Ok(Self(window))
    }
}

impl Drop for Listener<'_> {
    fn drop(&mut self) {
        // SAFETY: the window is still live because the listener borrows it.
        unsafe { RemoveClipboardFormatListener(self.0.0) };
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_CLIPBOARDUPDATE {
        // SAFETY: window is the live window this procedure serves; posting takes no pointers.
        unsafe { PostMessageW(window, CHANGED, 0, 0) };
        return 0;
    }
    // SAFETY: forwards the arguments Windows passed to this procedure.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn error_kind(error: &ClipboardError) -> &'static str {
    match error {
        ClipboardError::Busy => "busy",
        ClipboardError::Unsupported => "unsupported",
        ClipboardError::Malformed => "malformed",
        ClipboardError::Os(_) => "os",
    }
}

fn os_error(context: &str) -> ClipboardError {
    ClipboardError::Os(format!("{context}: {}", std::io::Error::last_os_error()))
}
