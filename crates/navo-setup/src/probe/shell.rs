//! IW40 shell probe: finds the desktop shell's user token and impersonates that user on a thread.
//! An elevated start does its file work as the user through `as_user` (IW34, IW39).

#![cfg(windows)]
#![allow(
    unsafe_code,
    reason = "the shell window, process tokens and thread impersonation require Win32 FFI"
)]

use std::cell::Cell;
use std::ffi::c_void;
use std::io::{self, Write};
use std::mem::size_of;
use std::os::windows::io::{AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle};
use std::ptr::{null, null_mut};
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_INSUFFICIENT_BUFFER, ERROR_INVALID_PARAMETER, HANDLE, HWND,
    LocalFree,
};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::{
    DuplicateTokenEx, GetTokenInformation, RevertToSelf, SecurityImpersonation, TOKEN_DUPLICATE,
    TOKEN_IMPERSONATE, TOKEN_QUERY, TOKEN_USER, TokenImpersonation, TokenSessionId, TokenUser,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, OpenProcessToken, PROCESS_CREATE_PROCESS, PROCESS_QUERY_LIMITED_INFORMATION,
    SetThreadToken,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId};

use super::shell_token;
use crate::admin_switch::{Identity, as_admin_with};
use crate::start_mode::ShellFacts;

/// The shell after `install`; set once per process.
static SHELL: OnceLock<Shell> = OnceLock::new();

thread_local! {
    /// This thread's impersonation: `None` never tried, `Some(true)` entered, `Some(false)` failed.
    static ENTERED: Cell<Option<bool>> = const { Cell::new(None) };
}

/// What `Shell::find` saw: no shell, a shell Setup may not open, or the shell itself.
#[derive(Debug)]
pub enum Found {
    Missing,
    Foreign,
    Shell(Shell),
}

/// The desktop shell: its process handle, an impersonation copy of its token, and its facts.
#[derive(Debug)]
pub struct Shell {
    process: OwnedHandle,
    token: OwnedHandle,
    facts: ShellFacts,
}

impl Shell {
    /// Finds the shell by its window and reads its token through the held process handle (IW40).
    /// Access denied is `Foreign`, a vanished shell is `Missing`; other failures are `Err`.
    pub fn find() -> io::Result<Found> {
        let Some(seen) = shell_window() else {
            return Ok(Found::Missing);
        };
        let process = match open_process(seen.1) {
            Ok(process) => process,
            Err(error) if is_code(&error, ERROR_ACCESS_DENIED) => return Ok(Found::Foreign),
            Err(error) if is_code(&error, ERROR_INVALID_PARAMETER) => return Ok(Found::Missing),
            Err(error) => return Err(error),
        };
        // The second read proves the held handle is the shell's, not a reused process id.
        if shell_window() != Some(seen) {
            return Ok(Found::Missing);
        }
        let primary = match open_token(&process) {
            Ok(token) => token,
            Err(error) if is_code(&error, ERROR_ACCESS_DENIED) => return Ok(Found::Foreign),
            Err(error) => return Err(error),
        };
        let facts = ShellFacts {
            sid: token_sid(&primary)?,
            session: token_session(&primary)?,
        };
        let token = impersonation_copy(&primary)?;
        Ok(Found::Shell(Shell {
            process,
            token,
            facts,
        }))
    }

    /// The shell user's SID and session.
    pub fn facts(&self) -> &ShellFacts {
        &self.facts
    }

    /// The shell process handle, the parent of every child Setup starts as the user (IW34).
    pub fn process(&self) -> BorrowedHandle<'_> {
        self.process.as_handle()
    }

    /// The impersonation copy of the shell user's token (query and impersonate rights).
    pub(super) fn token(&self) -> BorrowedHandle<'_> {
        self.token.as_handle()
    }
}

/// Stores the shell for the whole process; a second call is `AlreadyExists` and drops its shell.
pub fn install(shell: Shell) -> io::Result<&'static Shell> {
    SHELL.set(shell).map_err(|_| {
        io::Error::new(io::ErrorKind::AlreadyExists, "a shell is already installed")
    })?;
    SHELL
        .get()
        .ok_or_else(|| io::Error::other("the installed shell is missing"))
}

/// The installed shell, or `None` for a normal start.
pub fn installed() -> Option<&'static Shell> {
    SHELL.get()
}

/// Impersonates the shell user on this thread; with no shell installed it does nothing.
/// Fails with the `SetThreadToken` error, or when Windows lowered the token below impersonation
/// (then it reverts first, and aborts if that revert fails); either failure is recorded.
pub fn enter_as_user() -> io::Result<()> {
    let Some(shell) = installed() else {
        return Ok(());
    };
    // SAFETY: a null thread pointer means the calling thread; the token handle is live.
    let set = unsafe { SetThreadToken(null(), shell.token.as_raw_handle()) };
    if set == 0 {
        let error = io::Error::last_os_error();
        ENTERED.set(Some(false));
        return Err(error);
    }
    // Without SeImpersonatePrivilege the kernel may quietly drop the thread to identification.
    if let Err(error) = shell_token::thread_level().and_then(shell_token::require_impersonation) {
        ENTERED.set(Some(false));
        // SAFETY: no arguments; ends the unusable impersonation set just above.
        if unsafe { RevertToSelf() } == 0 {
            abort_on_identity(
                "could not stop a lowered impersonation",
                &io::Error::last_os_error(),
            );
        }
        return Err(error);
    }
    ENTERED.set(Some(true));
    Ok(())
}

/// Runs `f` as the shell user and reverts afterwards, also when `f` panics (IW39).
/// Runs `f` directly with no shell or when already entered; fails without running `f` if entering fails.
/// Threads spawned inside `f` do not inherit the impersonation; they run as Setup's admin token.
pub fn as_user<T>(f: impl FnOnce() -> T) -> io::Result<T> {
    if installed().is_none() || is_entered() {
        return Ok(f());
    }
    let before = ENTERED.get();
    enter_as_user()?;
    let _revert = Revert { before };
    Ok(f())
}

/// Runs `f` on Setup's own token, then impersonates again if this thread was entered. A failed
/// revert, or a failed re-entry after `f` or while `f` panics, aborts: the thread's identity would
/// be wrong. The `Result` stays for the contract's callers. A panic hook or a destructor inside `f`
/// still runs on the admin token, so keep user-side cleanup outside `f`.
pub fn as_admin<T>(f: impl FnOnce() -> T) -> io::Result<T> {
    as_admin_with(&ThreadIdentity, f)
}

/// Whether this thread is impersonating the shell user.
pub fn is_entered() -> bool {
    ENTERED.get() == Some(true)
}

/// Ends `as_user`'s impersonation on drop and restores the thread's earlier state.
struct Revert {
    before: Option<bool>,
}

impl Drop for Revert {
    /// Reverts to Setup's own token; a failed revert aborts, since the thread's identity is unknown.
    fn drop(&mut self) {
        // SAFETY: no arguments; ends this thread's impersonation.
        if unsafe { RevertToSelf() } == 0 {
            abort_on_identity(
                "could not stop acting as the desktop user",
                &io::Error::last_os_error(),
            );
        }
        ENTERED.set(self.before);
    }
}

/// This thread's identity as `as_admin` switches it.
struct ThreadIdentity;

impl Identity for ThreadIdentity {
    /// Reverts to Setup's own token; returns whether the thread was entered.
    fn revert(&self) -> io::Result<bool> {
        let was_entered = is_entered();
        // SAFETY: no arguments; ends this thread's impersonation, if any.
        if unsafe { RevertToSelf() } == 0 {
            return Err(io::Error::last_os_error());
        }
        if was_entered {
            ENTERED.set(None);
        }
        Ok(was_entered)
    }

    /// Impersonates the shell user again.
    fn enter(&self) -> io::Result<()> {
        enter_as_user()
    }

    /// Aborts: the thread's token is wrong or unknown and it must not run on.
    fn fatal(&self, error: io::Error) -> ! {
        abort_on_identity(
            "could not switch between the desktop user and Setup",
            &error,
        )
    }
}

/// Writes one stderr line and aborts when this thread's identity is wrong or unknown (IW39). The
/// write ignores its own failure, so nothing can unwind before the abort.
fn abort_on_identity(what: &str, error: &io::Error) -> ! {
    let _ = writeln!(io::stderr().lock(), "navo-setup: {what}: {error}");
    std::process::abort()
}

/// The shell window and its process id, or `None` when there is no shell.
fn shell_window() -> Option<(HWND, u32)> {
    // SAFETY: no arguments; returns null when no shell window exists.
    let window = unsafe { GetShellWindow() };
    if window.is_null() {
        return None;
    }
    let mut pid = 0u32;
    // SAFETY: `window` came from GetShellWindow and `pid` is a writable u32.
    unsafe { GetWindowThreadProcessId(window, &raw mut pid) };
    (pid != 0).then_some((window, pid))
}

/// Opens the shell process with the rights the spawn needs (query and create-process).
fn open_process(pid: u32) -> io::Result<OwnedHandle> {
    let rights = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_CREATE_PROCESS;
    // SAFETY: value arguments only; a null result is read as the last error right after.
    let raw = unsafe { OpenProcess(rights, 0, pid) };
    owned(raw)
}

/// Opens the process token for reading and duplicating.
fn open_token(process: &OwnedHandle) -> io::Result<OwnedHandle> {
    let mut raw: HANDLE = null_mut();
    let rights = TOKEN_QUERY | TOKEN_DUPLICATE;
    // SAFETY: the process handle is live and `raw` is a writable handle slot.
    let opened = unsafe { OpenProcessToken(process.as_raw_handle(), rights, &raw mut raw) };
    if opened == 0 {
        return Err(io::Error::last_os_error());
    }
    owned(raw)
}

/// Duplicates the primary token as an impersonation token at `SecurityImpersonation`.
fn impersonation_copy(primary: &OwnedHandle) -> io::Result<OwnedHandle> {
    let mut raw: HANDLE = null_mut();
    // SAFETY: the source token is live, no attributes are passed and `raw` is a writable slot.
    let copied = unsafe {
        DuplicateTokenEx(
            primary.as_raw_handle(),
            TOKEN_QUERY | TOKEN_IMPERSONATE,
            null(),
            SecurityImpersonation,
            TokenImpersonation,
            &raw mut raw,
        )
    };
    if copied == 0 {
        return Err(io::Error::last_os_error());
    }
    owned(raw)
}

/// Reads the token's session id (`TokenSessionId`).
fn token_session(token: &OwnedHandle) -> io::Result<u32> {
    let mut session = 0u32;
    let mut returned = 0u32;
    let size = u32::try_from(size_of::<u32>()).map_err(io::Error::other)?;
    let info = (&raw mut session).cast::<c_void>();
    // SAFETY: the token is live and `session` is a writable u32 of the size passed.
    let read = unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenSessionId,
            info,
            size,
            &raw mut returned,
        )
    };
    if read == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(session)
}

/// Reads the token's user SID (`TokenUser`) as an `S-1-...` string.
fn token_sid(token: &OwnedHandle) -> io::Result<String> {
    let mut needed = 0u32;
    // SAFETY: a null buffer of length zero only asks for the size.
    let sized = unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            null_mut(),
            0,
            &raw mut needed,
        )
    };
    if sized == 0 {
        let error = io::Error::last_os_error();
        if !is_code(&error, ERROR_INSUFFICIENT_BUFFER) {
            return Err(error);
        }
    }
    let bytes = usize::try_from(needed)
        .map_err(io::Error::other)?
        .max(size_of::<TOKEN_USER>());
    // u64 units keep the buffer aligned for TOKEN_USER and its SID.
    let mut buffer = vec![0u64; bytes.div_ceil(size_of::<u64>())];
    let size = u32::try_from(buffer.len() * size_of::<u64>()).map_err(io::Error::other)?;
    // SAFETY: the token is live and `buffer` is a writable, aligned block of `size` bytes.
    let read = unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast::<c_void>(),
            size,
            &raw mut needed,
        )
    };
    if read == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the call filled `buffer` with a TOKEN_USER whose SID points inside it.
    let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    sid_string(sid)
}

/// Converts a SID to its string form and frees the system's copy.
fn sid_string(sid: *mut c_void) -> io::Result<String> {
    let mut text: *mut u16 = null_mut();
    // SAFETY: `sid` points to a valid SID and `text` is a writable pointer slot.
    if unsafe { ConvertSidToStringSidW(sid, &raw mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let local = LocalText(text);
    let mut len = 0usize;
    // SAFETY: the string is NUL-terminated and stays live until `local` drops.
    while unsafe { *local.0.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: `len` units before the NUL were just read from the same live string.
    let units = unsafe { std::slice::from_raw_parts(local.0, len) };
    String::from_utf16(units).map_err(io::Error::other)
}

/// A string allocated by the system with `LocalAlloc`, freed on drop.
struct LocalText(*mut u16);

impl Drop for LocalText {
    /// Frees the string once.
    fn drop(&mut self) {
        // SAFETY: the pointer came from ConvertSidToStringSidW and is freed exactly once.
        unsafe { LocalFree(self.0.cast::<c_void>()) };
    }
}

/// Wraps a fresh handle in an owner that closes it; null is the last OS error.
fn owned(raw: HANDLE) -> io::Result<OwnedHandle> {
    if raw.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `raw` is a fresh handle this process owns; the wrapper closes it exactly once.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}

/// Whether an I/O error carries the given Win32 error code.
fn is_code(error: &io::Error, code: u32) -> bool {
    error.raw_os_error().and_then(|raw| u32::try_from(raw).ok()) == Some(code)
}

#[cfg(test)]
mod tests {
    use super::{Found, Shell, as_admin, as_user, enter_as_user, installed, is_entered};
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;

    #[test]
    fn nothing_installed_enter_is_ok_and_not_entered() {
        assert!(installed().is_none());
        assert!(enter_as_user().is_ok());
        assert!(!is_entered());
    }

    #[test]
    fn nothing_installed_as_user_and_as_admin_run_directly() {
        assert_eq!(as_user(|| 4).ok(), Some(4));
        assert_eq!(as_admin(|| 5).ok(), Some(5));
        assert!(!is_entered());
    }

    #[test]
    fn find_returns_ok_on_the_runner() {
        let found = Shell::find();
        assert!(found.is_ok(), "{found:?}");
        if let Ok(Found::Shell(shell)) = found {
            assert!(shell.facts().sid.starts_with("S-1-"), "{:?}", shell.facts());
            let mut session = u32::MAX;
            // SAFETY: value argument and a writable u32 slot.
            let read = unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &raw mut session) };
            assert_ne!(read, 0, "{}", std::io::Error::last_os_error());
            assert_eq!(shell.facts().session, session);
        }
    }
}
