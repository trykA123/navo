//! Native crash client (Windows): starts this GUI's hidden `--crash-server` process and asks it
//! for a minidump when an unhandled exception reaches the process (RP2). Panics stay with `hook`.

use std::ffi::OsStr;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::ExitStatusExt;
use std::path::Path;
use std::process::ExitStatus;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use std::{fs, io, ptr, thread};

use crash_handler::{CrashEventResult, CrashHandler, make_crash_event};
use minidumper::{Client, SocketName};
use windows_sys::Win32::Foundation::{EXCEPTION_ACCESS_VIOLATION, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Diagnostics::Debug::{EXCEPTION_RECORD, RtlRaiseException};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, GetExitCodeProcess,
    PROCESS_INFORMATION, STARTUPINFOW, TerminateProcess, WaitForSingleObject,
};

/// The attached handler; set once, when the client is connected to its server.
static NATIVE: OnceLock<CrashHandler> = OnceLock::new();
/// Guards against a second `attach`, which would start a second server.
static STARTED: AtomicBool = AtomicBool::new(false);
/// Set when the attach thread gives up, so `ready` stops waiting at once.
static FAILED: AtomicBool = AtomicBool::new(false);

const CONNECT_TRIES: u32 = 50;
const CONNECT_PAUSE: Duration = Duration::from_millis(100);
/// The first retry comes back quickly, while the server is still starting.
const CONNECT_FIRST_PAUSE: Duration = Duration::from_millis(10);
/// How long `ready` waits: longer than the connect budget plus a graceful server stop.
pub(crate) const ATTACH_BUDGET: Duration = Duration::from_secs(9);
/// An attached client closes first, so the server can remove its own socket.
const CLOSE_GRACE: Duration = Duration::from_secs(1);
/// A bounded wait after a kill, so a stuck server never hangs the attach thread.
const REAP_WAIT_MS: u32 = 2_000;
/// `AF_UNIX` paths hold 108 bytes on Windows; RP2 keeps a margin.
const MAX_SOCKET_BYTES: usize = 100;
/// `EXCEPTION_NONCONTINUABLE`: the faulting code must never resume.
const NONCONTINUABLE: u32 = 1;

/// Starts the native crash reporter for dumps into `logs` on a new thread; never blocks.
/// Any failure logs one `warn` line and leaves only the panic hook in place.
pub(super) fn attach(logs: &Path) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let logs = logs.to_path_buf();
    let spawned = thread::Builder::new()
        .name("crash-client".into())
        .spawn(move || {
            if let Err(reason) = connect(&logs) {
                FAILED.store(true, Ordering::SeqCst);
                tracing::warn!(%reason, "native crash reporter off");
            }
        });
    if let Err(error) = spawned {
        FAILED.store(true, Ordering::SeqCst);
        tracing::warn!(reason = %error, "native crash reporter off");
    }
}

/// Spawns the server, connects and attaches the handler; the error names the failed step.
fn connect(logs: &Path) -> Result<(), String> {
    let socket = std::env::temp_dir().join(format!("navo-crash-{}.sock", std::process::id()));
    if socket.as_os_str().len() > MAX_SOCKET_BYTES {
        return Err("socket path too long".into());
    }
    remove_stale(&socket).map_err(|error| format!("stale socket: {}", error.kind()))?;
    let server =
        spawn_server(&socket, logs).map_err(|error| format!("server spawn: {}", error.kind()))?;
    let client = match connect_client(&socket, &server) {
        Ok(client) => client,
        Err(reason) => {
            stop(&server, &socket, Duration::ZERO);
            return Err(reason);
        }
    };
    // SAFETY: the closure only sends one dump request on the socket that is already connected;
    // it takes no lock and starts nothing, so it is sound in the exception filter.
    let event = unsafe {
        make_crash_event(move |context| {
            CrashEventResult::Handled(client.request_dump(context).is_ok())
        })
    };
    let handler = match CrashHandler::attach(event) {
        Ok(handler) => handler,
        Err(error) => {
            // Attach consumed the event, which closed the client, so the server can exit cleanly.
            stop(&server, &socket, CLOSE_GRACE);
            return Err(format!("attach: {error}"));
        }
    };
    // `STARTED` makes this the only `set`, so it cannot fail.
    let _ = NATIVE.set(handler);
    Ok(())
}

/// Removes a socket file left by an earlier process with this pid; a missing file is fine.
fn remove_stale(socket: &Path) -> io::Result<()> {
    match fs::remove_file(socket) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// The hidden crash-server process, owned through its Windows process handle.
struct Server {
    process: OwnedHandle,
}

impl Server {
    /// Returns the exit status when the server has exited; fails when Windows refuses the query.
    fn try_wait(&self) -> io::Result<Option<ExitStatus>> {
        // SAFETY: `self.process` is a live process handle owned by this value; zero waits not at all.
        match unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } {
            WAIT_OBJECT_0 => Ok(Some(self.status()?)),
            WAIT_TIMEOUT => Ok(None),
            _ => Err(io::Error::last_os_error()),
        }
    }

    /// Waits up to two seconds for an exiting server and returns its status.
    fn wait(&self) -> io::Result<ExitStatus> {
        // SAFETY: `self.process` is a live process handle owned by this value.
        match unsafe { WaitForSingleObject(self.process.as_raw_handle(), REAP_WAIT_MS) } {
            WAIT_OBJECT_0 => self.status(),
            WAIT_TIMEOUT => Err(io::Error::from(io::ErrorKind::TimedOut)),
            _ => Err(io::Error::last_os_error()),
        }
    }

    /// Reads the process exit code after the process has exited.
    fn status(&self) -> io::Result<ExitStatus> {
        let mut code = 0;
        // SAFETY: `self.process` is live and has exited; `code` is a writable `u32`.
        if unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(ExitStatus::from_raw(code))
    }

    /// Terminates the server; a process that already exited is not an error.
    fn kill(&self) -> io::Result<()> {
        // SAFETY: `self.process` is a live process handle with terminate access.
        if unsafe { TerminateProcess(self.process.as_raw_handle(), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

/// Starts `navo.exe --crash-server <socket> <logs>` without inheriting any of the GUI's handles:
/// `CreateProcessW` gets `bInheritHandles = 0`, so test pipes and console handles stay closed.
fn spawn_server(socket: &Path, logs: &Path) -> io::Result<Server> {
    let exe = std::env::current_exe()?;
    let mut line = Vec::new();
    for arg in [
        exe.as_os_str(),
        OsStr::new("--crash-server"),
        socket.as_os_str(),
        logs.as_os_str(),
    ] {
        quote_arg(&mut line, arg);
    }
    line.push(0);
    let exe = wide(&exe);
    let startup = STARTUPINFOW {
        cb: u32::try_from(size_of::<STARTUPINFOW>())
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?,
        ..STARTUPINFOW::default()
    };
    let mut process = PROCESS_INFORMATION::default();
    // SAFETY: both UTF-16 buffers are NUL-terminated and live across the call, the startup and
    // process structures are writable, and no security attributes or environment block are read.
    let created = unsafe {
        CreateProcessW(
            exe.as_ptr(),
            line.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            0,
            CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
            ptr::null(),
            ptr::null(),
            &startup,
            &mut process,
        )
    };
    if created == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: CreateProcessW returned these two live handles and nothing else owns them.
    drop(unsafe { OwnedHandle::from_raw_handle(process.hThread) });
    // SAFETY: as above; the process handle is now owned by `Server` and closed on drop.
    let process = unsafe { OwnedHandle::from_raw_handle(process.hProcess) };
    Ok(Server { process })
}

/// Encodes `path` as a NUL-terminated UTF-16 string for a Windows API call.
fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain([0]).collect()
}

/// Appends `arg` to a command line, quoted and escaped by the `CreateProcessW` rules.
fn quote_arg(line: &mut Vec<u16>, arg: &OsStr) {
    let units: Vec<u16> = arg.encode_wide().collect();
    let needs_quotes = units.is_empty()
        || units.iter().any(|&unit| {
            unit == u16::from(b' ') || unit == u16::from(b'\t') || unit == u16::from(b'"')
        });
    if !needs_quotes {
        line.extend_from_slice(&units);
        line.push(u16::from(b' '));
        return;
    }
    line.push(u16::from(b'"'));
    let mut backslashes = 0;
    for unit in units {
        if unit == u16::from(b'\\') {
            backslashes += 1;
            continue;
        }
        // Backslashes only need doubling before a quote or the closing quote.
        let escape = if unit == u16::from(b'"') {
            backslashes * 2 + 1
        } else {
            backslashes
        };
        line.extend(std::iter::repeat_n(u16::from(b'\\'), escape));
        line.push(unit);
        backslashes = 0;
    }
    line.extend(std::iter::repeat_n(u16::from(b'\\'), backslashes * 2));
    line.push(u16::from(b'"'));
    line.push(u16::from(b' '));
}

/// Connects to the server, 50 tries apart: 10 ms first, then 100 ms; fails when it already exited.
fn connect_client(socket: &Path, server: &Server) -> Result<Client, String> {
    let mut last = String::from("no try");
    for attempt in 0..CONNECT_TRIES {
        match Client::with_name(SocketName::Path(socket)) {
            Ok(client) => return Ok(client),
            Err(error) => last = error.to_string(),
        }
        if let Ok(Some(status)) = server.try_wait() {
            return Err(format!("server exited: {status}"));
        }
        thread::sleep(if attempt == 0 {
            CONNECT_FIRST_PAUSE
        } else {
            CONNECT_PAUSE
        });
    }
    Err(format!("connect: {last}"))
}

/// Stops the server and removes its socket; `grace` lets a client close first, so the server
/// removes its own socket before the fallback kill.
fn stop(server: &Server, socket: &Path, grace: Duration) {
    let deadline = Instant::now() + grace;
    while server.try_wait().ok().flatten().is_none() {
        if Instant::now() >= deadline {
            if let Err(error) = server.kill() {
                tracing::warn!(kind = %error.kind(), "could not stop the crash server");
            }
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    if let Err(error) = server.wait() {
        tracing::warn!(kind = %error.kind(), "could not reap the crash server");
    }
    if let Err(error) = remove_stale(socket) {
        tracing::warn!(kind = %error.kind(), "could not remove the crash socket");
    }
}

/// Waits up to `timeout` for the reporter to attach, checking every 10 ms; true once attached.
/// Returns false early when the attach thread has already given up.
pub(crate) fn ready(timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if NATIVE.get().is_some() {
            return true;
        }
        if FAILED.load(Ordering::SeqCst) {
            return false;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// Raises a non-continuable access violation from this function, for the CI crash probe.
/// `RtlRaiseException` records its caller as the fault address (inside navo.exe), while
/// `RaiseException` would record its own frame in `KernelBase.dll`.
#[inline(never)]
pub(crate) fn raise_access_violation() -> ! {
    let mut record = EXCEPTION_RECORD {
        ExceptionCode: EXCEPTION_ACCESS_VIOLATION,
        ExceptionFlags: NONCONTINUABLE,
        ..EXCEPTION_RECORD::default()
    };
    // SAFETY: `record` is a valid, writable exception record that lives across the call; the
    // OS fills in its address field and dispatches the exception like a real fault.
    unsafe { RtlRaiseException(std::ptr::addr_of_mut!(record).cast_const()) };
    std::process::abort()
}
