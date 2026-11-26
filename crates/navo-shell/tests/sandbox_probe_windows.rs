//! Windows AppContainer hostile-probe integration test (S10).
//!
//! The parent spawns a copy of itself through `spawn_confined` with the full sandbox policy
//! (zero-capability AppContainer, Job UI limits, mitigations). The child attempts four hostile
//! actions and prints one machine-readable line each; the parent asserts all four report `denied`.
//!
//! This binary uses `harness = false` (set in `Cargo.toml`) so the probe child runs directly
//! from `fn main()` without libtest touching console/threads inside AppContainer.

#[cfg(windows)]
mod probe {
    #![allow(
        clippy::collapsible_if,
        clippy::expect_used,
        clippy::excessive_nesting,
        clippy::panic,
        clippy::print_stdout,
        clippy::unwrap_used,
        clippy::unused_io_amount,
        reason = "test harness: expect/unwrap on test setup; println communicates probe results; partial read is intentional; module adds a nesting level; panic on bad child args is fine in tests"
    )]

    use std::env;
    use std::fs::File;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::os::windows::io::AsRawHandle;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use navo_shell::{HelperLimits, spawn_confined};
    use tempfile::TempDir;
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
    use windows_sys::Win32::System::Ole::CF_TEXT;
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};

    const LIMITS: HelperLimits = HelperLimits {
        memory_bytes: 256 << 20,
    };

    /// Marker values the child prints; the parent counts them.
    const DENIED: &str = "denied";
    const ALLOWED: &str = "allowed";

    pub fn run() {
        if let Some(port) = find_probe_port() {
            let _ = std::io::stdout().flush();
            run_probe_child(port);
            let _ = std::io::stdout().flush();
            return;
        }
        if handle_list_args() {
            return;
        }

        let dir = prepare_probe_exe();
        let listener = bind_loopback_listener();
        let port = listener.local_addr().unwrap().port();

        let args = probe_child_args(port);
        let child = spawn_probe_child(&dir, &args);
        let child = child.expect("spawn_confined must start the probe child");

        let lines = read_child_stdout(child.stdout);
        let handle = child.process.process_handle().as_raw_handle();
        // SAFETY: `handle` is the live child process handle; 5 s is plenty for a probe that
        // already finished writing to its stdout pipe.
        unsafe { WaitForSingleObject(handle, 5000) };
        let mut exit_code = 1u32;
        // SAFETY: `handle` is the same live process handle with PROCESS_QUERY_INFORMATION.
        unsafe { GetExitCodeProcess(handle, &mut exit_code) };
        let _ = child.process.kill();

        drop(listener);

        let mut denied = 0;
        let mut allowed = 0;
        for line in &lines {
            if line.ends_with(DENIED) {
                denied += 1;
            } else if line.ends_with(ALLOWED) {
                allowed += 1;
            }
        }
        assert_eq!(
            denied, 4,
            "expected 4 denied lines, got: {lines:?} (exit_code=0x{exit_code:08x})"
        );
        assert_eq!(allowed, 0, "no probe must succeed: {lines:?}");
    }

    /// Scans `--probe-port <port>` from the command line; `Some` means this process is the
    /// probe child (the parent passes the port on the CLI, no env var needed).
    fn find_probe_port() -> Option<u16> {
        env::args()
            .collect::<Vec<_>>()
            .windows(2)
            .find(|w| w[0] == "--probe-port")
            .and_then(|w| w[1].parse().ok())
    }

    /// Speaks the libtest `--list` protocol so `cargo nextest` discovery does not spawn a child.
    fn handle_list_args() -> bool {
        let args: Vec<String> = env::args().collect();
        if !args.iter().any(|arg| arg == "--list") {
            return false;
        }
        if !args.iter().any(|arg| arg == "--ignored") {
            println!("sandbox_probe_denies_hostile_actions: test");
        }
        true
    }

    /// Command-line arguments for the probe child: port on the command line avoids any
    /// env-stripping issues inside AppContainer.
    fn probe_child_args(port: u16) -> Vec<std::ffi::OsString> {
        vec!["--probe-port".into(), port.to_string().into()]
    }

    /// A test-scoped copy of this binary beside a stub `pdfium.dll`, because `spawn_confined`
    /// grants the AppContainer read+execute on the exe and its sibling dll (S2c2).
    struct ProbeExe {
        _dir: TempDir,
        exe: PathBuf,
    }

    fn prepare_probe_exe() -> ProbeExe {
        let dir = TempDir::new().expect("tempdir for probe exe");
        let exe = dir.path().join("probe.exe");
        let self_exe = env::current_exe().expect("current_exe");
        std::fs::copy(&self_exe, &exe).expect("copy self into tempdir");
        std::fs::write(dir.path().join("pdfium.dll"), b"stub").expect("write stub pdfium.dll");
        ProbeExe { _dir: dir, exe }
    }

    fn bind_loopback_listener() -> TcpListener {
        TcpListener::bind(("127.0.0.1", 0)).expect("bind loopback listener")
    }

    fn spawn_probe_child(
        dir: &ProbeExe,
        args: &[std::ffi::OsString],
    ) -> std::io::Result<navo_shell::ConfinedChild> {
        spawn_confined(&dir.exe, args, LIMITS)
    }

    fn read_child_stdout(stdout: File) -> Vec<String> {
        let mut reader = BufReader::new(stdout);
        let mut lines = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let trimmed = line.trim_end().to_string();
                    if !trimmed.is_empty() {
                        lines.push(trimmed);
                    }
                }
                Err(_) if Instant::now() > deadline => break,
                Err(_) => break,
            }
        }
        lines
    }

    /// The probe child: attempts four hostile actions and prints one line each.
    fn run_probe_child(port: u16) {
        let gitconfig = probe_gitconfig();
        println!("gitconfig: {gitconfig}");

        let smb = probe_loopback_tcp(port);
        println!("smb: {smb}");

        let clipboard = probe_clipboard();
        println!("clipboard: {clipboard}");

        let cmd = probe_cmd_spawn();
        println!("cmd: {cmd}");
    }

    /// Attempts to read `%USERPROFILE%\.gitconfig` (or the directory if the file is absent);
    /// reports `allowed` only on a successful read or listing.
    fn probe_gitconfig() -> &'static str {
        let profile = match env::var_os("USERPROFILE") {
            Some(p) => PathBuf::from(p),
            None => return DENIED,
        };
        let gitconfig = profile.join(".gitconfig");
        if gitconfig.is_file() {
            match File::open(&gitconfig) {
                Ok(mut f) => {
                    let mut buf = [0u8; 1];
                    match f.read(&mut buf) {
                        Ok(_) => ALLOWED,
                        Err(_) => DENIED,
                    }
                }
                Err(_) => DENIED,
            }
        } else {
            match std::fs::read_dir(&profile) {
                Ok(_) => ALLOWED,
                Err(_) => DENIED,
            }
        }
    }

    /// Attempts a TCP connect to the port the parent bound; reports `allowed` only on success.
    fn probe_loopback_tcp(port: u16) -> &'static str {
        match TcpStream::connect_timeout(
            &format!("127.0.0.1:{port}").parse().unwrap(),
            Duration::from_secs(2),
        ) {
            Ok(_) => ALLOWED,
            Err(_) => DENIED,
        }
    }

    /// Attempts to open the clipboard and read `CF_TEXT`; reports `allowed` only on success.
    ///
    /// `user32.dll` is loaded dynamically: a static import links it at process start, and in a
    /// zero-capability AppContainer on a non-interactive WindowStation the DLL's `DllMain` fails
    /// with `STATUS_DLL_INIT_FAILED` (0xC0000142) before `main()` even runs.
    fn probe_clipboard() -> &'static str {
        type OpenClipboardFn = unsafe extern "system" fn(*mut std::ffi::c_void) -> i32;
        type GetClipboardDataFn = unsafe extern "system" fn(u32) -> *mut std::ffi::c_void;
        type CloseClipboardFn = unsafe extern "system" fn() -> i32;

        // SAFETY: `LoadLibraryA` with a valid C string; returns null when AppContainer blocks
        // `user32.dll` init on a non-interactive WindowStation.
        let hmod = unsafe { LoadLibraryA(c"user32.dll".as_ptr().cast()) };
        if hmod.is_null() {
            return DENIED;
        }
        // SAFETY: resolving `OpenClipboard` from a successfully loaded `user32.dll`.
        let open = unsafe { GetProcAddress(hmod, c"OpenClipboard".as_ptr().cast()) };
        // SAFETY: resolving `GetClipboardData` from the same loaded module.
        let get_data = unsafe { GetProcAddress(hmod, c"GetClipboardData".as_ptr().cast()) };
        // SAFETY: resolving `CloseClipboard` from the same loaded module.
        let close = unsafe { GetProcAddress(hmod, c"CloseClipboard".as_ptr().cast()) };
        let (Some(open), Some(get_data), Some(close)) = (open, get_data, close) else {
            return DENIED;
        };
        // SAFETY: `open` is the real `OpenClipboard` from `user32.dll`.
        let open_fn: OpenClipboardFn = unsafe { std::mem::transmute(open) };
        // SAFETY: `get_data` is the real `GetClipboardData` from `user32.dll`.
        let get_data_fn: GetClipboardDataFn = unsafe { std::mem::transmute(get_data) };
        // SAFETY: `close` is the real `CloseClipboard` from `user32.dll`.
        let close_fn: CloseClipboardFn = unsafe { std::mem::transmute(close) };
        // SAFETY: a null owner is permitted for clipboard queries.
        if unsafe { open_fn(std::ptr::null_mut()) } == 0 {
            return DENIED;
        }
        // SAFETY: this process opened the clipboard above.
        let data = unsafe { get_data_fn(u32::from(CF_TEXT)) };
        // SAFETY: this process opened the clipboard and must close it.
        unsafe { close_fn() };
        if data.is_null() { DENIED } else { ALLOWED }
    }

    /// Attempts to spawn `cmd.exe /c exit 0`; reports `allowed` only on a successful spawn.
    fn probe_cmd_spawn() -> &'static str {
        let cmd = env::var_os("ComSpec").unwrap_or_else(|| "C:\\Windows\\System32\\cmd.exe".into());
        match std::process::Command::new(cmd)
            .args(["/c", "exit", "0"])
            .spawn()
        {
            Ok(mut child) => {
                let _ = child.wait();
                ALLOWED
            }
            Err(_) => DENIED,
        }
    }
}

/// Entry point for the sandbox probe test binary (`harness = false`).
/// On Windows, runs the parent test or (when `--probe-port` is passed) the probe child.
#[cfg(windows)]
fn main() {
    probe::run();
}

/// On non-Windows this binary is a no-op (the test file is Windows-only).
#[cfg(not(windows))]
fn main() {}
