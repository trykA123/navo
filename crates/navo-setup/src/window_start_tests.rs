//! Windows-only test: Setup frees its console, so std handles go stale; launching Navo must still work.

use crate::window_worker::{Probe, WindowsProbe};
use windows_sys::Win32::System::Console::{
    GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
};

/// `FreeConsole` leaves the std handle numbers set but dead; `Command` inherits them and CreateProcess fails.
#[test]
fn start_navo_works_when_the_std_handles_are_stale() {
    let root = tempfile::tempdir().expect("temp root");
    let current = root.path().join("current");
    std::fs::create_dir(&current).expect("current dir");
    let system = std::env::var_os("SystemRoot").expect("SystemRoot");
    let whoami = std::path::Path::new(&system)
        .join("System32")
        .join("whoami.exe");
    std::fs::copy(&whoami, current.join("navo.exe")).expect("stand-in navo.exe");

    let kinds = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE];
    // SAFETY: reads and restores this process's std handle slots; the stale value is never used as a handle by us.
    let saved = kinds.map(|kind| unsafe { GetStdHandle(kind) });
    for kind in kinds {
        // SAFETY: 0x7ff4 names no open handle, which is what a freed console leaves behind.
        unsafe { SetStdHandle(kind, 0x7ff4 as _) };
    }
    let result = WindowsProbe.start_navo(root.path());
    for (kind, handle) in kinds.into_iter().zip(saved) {
        // SAFETY: puts back the handles read above.
        unsafe { SetStdHandle(kind, handle) };
    }
    assert!(result.is_ok(), "start_navo failed: {result:?}");
}
