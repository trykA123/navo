//! Starting the installed Navo so it outlives Setup's job object (Windows only).
//!
//! A child Setup spawns directly dies with it, so the launch either breaks away from the job or is
//! handed to Explorer, which starts Navo in Explorer's own process tree.

#![cfg(windows)]
#![allow(
    unsafe_code,
    reason = "the Explorer shell-automation chain needs COM and raw variant fields"
)]

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{
    CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize, IDispatch, IServiceProvider,
};
use windows::Win32::System::Variant::{
    VARENUM, VARIANT, VARIANT_0_0, VARIANT_0_0_0, VT_BSTR, VT_I4, VariantClear,
};
use windows::Win32::UI::Shell::{
    IShellBrowser, IShellDispatch2, IShellFolderViewDual, IShellView, IShellWindows,
    SID_STopLevelBrowser, SVGIO_BACKGROUND, SWC_DESKTOP, SWFO_NEEDDISPATCH, ShellWindows,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{BSTR, Error, IUnknown, Interface};

/// Starts `exe` outside Setup's job so it survives the window closing; falls back to the shell.
pub(super) fn start_detached(exe: &Path, profile: &Path) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let mut command = Command::new(exe);
    command
        .current_dir(profile)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_BREAKAWAY_FROM_JOB);
    match command.spawn() {
        Ok(_) => Ok(()),
        // A job that forbids breakaway is left for the shell, which starts Navo outside it.
        Err(error) if error.raw_os_error() == Some(5) => shell_start(exe, profile),
        Err(error) => Err(error),
    }
}

/// Asks Explorer's shell automation to start `exe` from `profile`, outside Setup's job.
///
/// Explorer's `Shell` automation coclass is registered in-proc only, so it cannot be created
/// against `CLSCTX_LOCAL_SERVER`; the desktop shell window is walked to Explorer's own dispatch
/// instead. COM is initialized for this worker thread and released again on every path.
fn shell_start(exe: &Path, profile: &Path) -> io::Result<()> {
    // SAFETY: CoInitializeEx initializes COM for this thread and touches no memory we own.
    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if initialized.is_err() && initialized != RPC_E_CHANGED_MODE {
        return Err(io::Error::other(format!(
            "COM init failed: {initialized:?}"
        )));
    }
    let started = start_in_explorer(exe, profile);
    if initialized.is_ok() {
        // SAFETY: this balances the successful CoInitializeEx above.
        unsafe { CoUninitialize() };
    }
    started
}

/// Runs one ShellExecute through Explorer, translating every COM failure into an I/O error.
fn start_in_explorer(exe: &Path, profile: &Path) -> io::Result<()> {
    let dispatch = desktop_dispatch().map_err(io_failure)?;
    let file = BSTR::from_wide(&wide(exe));
    let mut directory = bstr_variant(&wide(profile));
    let mut show = number_variant(SW_SHOWNORMAL.0);
    let no_arguments = VARIANT::default();
    let default_verb = VARIANT::default();
    // SAFETY: the dispatch is Explorer's live automation object, the file string and all four
    // variants are initialized and stay alive for the call, and the empty variants mean no
    // arguments and the default verb. Explorer starts Navo; nothing here is freed mid-call.
    let started =
        unsafe { dispatch.ShellExecute(&file, &no_arguments, &directory, &default_verb, &show) };
    clear(&mut directory);
    clear(&mut show);
    started.map_err(io_failure)
}

/// Walks the desktop shell window to Explorer's out-of-process `IShellDispatch2`.
fn desktop_dispatch() -> Result<IShellDispatch2, Error> {
    // SAFETY: ShellWindows is Explorer's local-server coclass and no outer object is aggregated.
    let shell_windows: IShellWindows =
        unsafe { CoCreateInstance(&ShellWindows, None::<&IUnknown>, CLSCTX_LOCAL_SERVER) }?;
    let location = VARIANT::default();
    let location_root = VARIANT::default();
    let mut window = 0i32;
    // SAFETY: both variants are live VT_EMPTY values and `window` is a writable i32 that receives
    // the desktop window handle; the need-dispatch flag asks for the window's automation object.
    let desktop: IDispatch = unsafe {
        shell_windows.FindWindowSW(
            &location,
            &location_root,
            SWC_DESKTOP,
            &mut window,
            SWFO_NEEDDISPATCH,
        )
    }?;
    let services: IServiceProvider = desktop.cast()?;
    // SAFETY: the service provider is live and asks Explorer for its top-level browser.
    let browser: IShellBrowser = unsafe { services.QueryService(&SID_STopLevelBrowser) }?;
    // SAFETY: the browser is live and returns the view the user is looking at.
    let view: IShellView = unsafe { browser.QueryActiveShellView() }?;
    // SAFETY: a live view answers for its background item with the scriptable folder view.
    let folder: IShellFolderViewDual = unsafe { view.GetItemObject(SVGIO_BACKGROUND) }?;
    // SAFETY: the folder view's application is Explorer itself, queried for the dispatch IID.
    let application = unsafe { folder.Application() }?;
    application.cast()
}

/// Frees what a variant owns; a plain number or an empty variant clears to nothing.
fn clear(variant: &mut VARIANT) {
    // SAFETY: the variant is an initialized VARIANT whose tag matches the value it carries, so
    // VariantClear reads the tag and frees only what that variant owns.
    let _ = unsafe { VariantClear(std::ptr::from_mut(variant)) };
}

/// A `VT_BSTR` variant owning `text`; the caller clears it.
fn bstr_variant(text: &[u16]) -> VARIANT {
    // A zeroed union keeps the bytes the tag does not name initialized, because the variant is
    // passed to COM by value.
    let mut value = VARIANT_0_0_0::default();
    // SAFETY: `bstrVal` is the field `VT_BSTR` names; the string Rust moved into it is owned by the
    // variant until `clear` frees it.
    unsafe { value.bstrVal = std::mem::ManuallyDrop::new(BSTR::from_wide(text)) };
    tagged(VT_BSTR, value)
}

/// A `VT_I4` variant carrying one number; a number owns nothing.
fn number_variant(number: i32) -> VARIANT {
    let mut value = VARIANT_0_0_0::default();
    // SAFETY: `lVal` is the field `VT_I4` names, and an i32 holds no allocation.
    unsafe { value.lVal = number };
    tagged(VT_I4, value)
}

/// A variant tagged `vt` that carries the already initialized union `value`.
fn tagged(vt: VARENUM, value: VARIANT_0_0_0) -> VARIANT {
    let mut variant = VARIANT::default();
    // SAFETY: the tag and its value are written together and `value` is fully initialized, so the
    // variant always describes the bytes it carries.
    unsafe {
        variant.Anonymous.Anonymous = std::mem::ManuallyDrop::new(VARIANT_0_0 {
            vt,
            wReserved1: 0,
            wReserved2: 0,
            wReserved3: 0,
            Anonymous: value,
        });
    }
    variant
}

/// The UTF-16 units of a path without the terminator, for `BSTR::from_wide`.
fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().collect()
}

/// Turns a COM failure into the I/O error the worker logs; Setup never crashes on one.
fn io_failure(error: Error) -> io::Error {
    io::Error::other(error.message())
}
