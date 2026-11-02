#![allow(unsafe_code, reason = "launching through the shell requires Win32 FFI")]

use std::io;
use std::ptr;

use windows_sys::Win32::Foundation::{ERROR_CANCELLED, RPC_E_CHANGED_MODE};
use windows_sys::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
};
use windows_sys::Win32::UI::Shell::{SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use super::LaunchRequest;

struct ComApartment {
    initialized: bool,
}

impl ComApartment {
    fn enter() -> Self {
        // SAFETY: a NULL reserved pointer is required; the flags are valid COINIT values.
        let result = unsafe {
            CoInitializeEx(
                ptr::null(),
                (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
            )
        };
        let initialized = result >= 0;
        if !initialized && result != RPC_E_CHANGED_MODE {
            tracing::warn!(result, "could not initialise COM on the launch worker");
        }
        Self { initialized }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: balances the successful CoInitializeEx on this same thread.
            unsafe { CoUninitialize() };
        }
    }
}

thread_local! {
    static APARTMENT: ComApartment = ComApartment::enter();
}

pub(super) fn start(request: LaunchRequest) {
    APARTMENT.with(|_| ());
    let file = wide(&request.file);
    let directory = request.directory.as_deref().map(wide);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC,
        lpFile: file.as_ptr(),
        lpDirectory: directory.as_ref().map_or(ptr::null(), |text| text.as_ptr()),
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };
    // SAFETY: file and directory stay NUL-terminated and alive until the synchronous call returns.
    if unsafe { ShellExecuteExW(&mut info) } != 0 {
        return;
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() != Some(ERROR_CANCELLED as i32) {
        tracing::warn!(%error, file = %request.file, "the shell could not open the file");
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
