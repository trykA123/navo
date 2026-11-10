//! Native crash client (Windows): starts this GUI's hidden `--crash-server` process and asks it
//! for a minidump when an unhandled exception reaches the process (RP2). Panics stay with `hook`.

use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use std::{fs, io, thread};

use crash_handler::{CrashEventResult, CrashHandler, make_crash_event};
use minidumper::{Client, SocketName};
use windows_sys::Win32::Foundation::EXCEPTION_ACCESS_VIOLATION;
use windows_sys::Win32::System::Diagnostics::Debug::{EXCEPTION_RECORD, RtlRaiseException};
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

/// The attached handler; set once, when the client is connected to its server.
static NATIVE: OnceLock<CrashHandler> = OnceLock::new();
/// Guards against a second `attach`, which would start a second server.
static STARTED: AtomicBool = AtomicBool::new(false);

const CONNECT_TRIES: u32 = 50;
const CONNECT_PAUSE: Duration = Duration::from_millis(100);
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
                tracing::warn!(%reason, "native crash reporter off");
            }
        });
    if let Err(error) = spawned {
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
    let mut server =
        spawn_server(&socket, logs).map_err(|error| format!("server spawn: {}", error.kind()))?;
    let client = match connect_client(&socket, &mut server) {
        Ok(client) => client,
        Err(reason) => {
            stop(&mut server);
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
    let handler = CrashHandler::attach(event).map_err(|error| format!("attach: {error}"))?;
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

/// Starts `navo.exe --crash-server <socket> <logs>` with no window and no stdio.
fn spawn_server(socket: &Path, logs: &Path) -> io::Result<Child> {
    Command::new(std::env::current_exe()?)
        .arg("--crash-server")
        .arg(socket)
        .arg(logs)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
}

/// Connects to the server, 50 tries 100 ms apart; fails early when the server already exited.
fn connect_client(socket: &Path, server: &mut Child) -> Result<Client, String> {
    let mut last = String::from("no try");
    for _ in 0..CONNECT_TRIES {
        match Client::with_name(SocketName::Path(socket)) {
            Ok(client) => return Ok(client),
            Err(error) => last = error.to_string(),
        }
        if let Ok(Some(status)) = server.try_wait() {
            return Err(format!("server exited: {status}"));
        }
        thread::sleep(CONNECT_PAUSE);
    }
    Err(format!("connect: {last}"))
}

/// Kills and reaps a server that never got a client, so it cannot outlive the GUI.
fn stop(server: &mut Child) {
    if let Err(error) = server.kill() {
        tracing::warn!(kind = %error.kind(), "could not stop the crash server");
    }
    let _ = server.wait();
}

/// Waits up to `timeout` for the reporter to attach, checking every 10 ms; true once attached.
pub(crate) fn ready(timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if NATIVE.get().is_some() {
            return true;
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
