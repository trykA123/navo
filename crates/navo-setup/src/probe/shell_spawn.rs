//! Starts a child as the desktop user: the shell process is its parent, so the child runs with the
//! shell user's token in the shell's session (IW34, H4). The child inherits no handles, and gets the
//! shell user's own environment and an explicit folder, never Setup's.

#![cfg(windows)]
#![allow(
    unsafe_code,
    reason = "the parent-process attribute and the process wait require Win32 FFI"
)]

use std::ffi::{OsStr, OsString, c_void};
use std::io;
use std::marker::PhantomData;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::ptr::{null, null_mut};
use std::time::Duration;

use windows_sys::Win32::Foundation::{HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows_sys::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, INFINITE, InitializeProcThreadAttributeList,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROC_THREAD_ATTRIBUTE_PARENT_PROCESS, PROCESS_INFORMATION,
    STARTUPINFOEXW, UpdateProcThreadAttribute, WaitForSingleObject,
};

use super::shell::Shell;
use crate::cmdline::{command_line, is_absolute};
use crate::runner::Child;

/// A child started by `spawn_as_shell`. Dropping it closes the handle and never kills the child.
#[derive(Debug)]
pub struct ShellChild {
    process: OwnedHandle,
}

/// Starts `program` with `args` in the folder `cwd` as the shell user, with the shell as its parent
/// process. No handles are inherited; the child gets the shell user's environment, not Setup's.
/// Fails with the Windows error, or `InvalidInput` when `program` or `cwd` is not an absolute
/// path, or a path or argument holds a NUL.
pub fn spawn_as_shell(
    shell: &Shell,
    program: &OsStr,
    args: &[OsString],
    cwd: &Path,
) -> io::Result<ShellChild> {
    let program = absolute_wide(program)?;
    let mut directory = absolute_wide(cwd.as_os_str())?;
    directory.push(0);
    let units = args
        .iter()
        .map(|arg| wide(arg.as_os_str()))
        .collect::<io::Result<Vec<_>>>()?;
    let mut line = command_line(&program, units.iter().map(Vec::as_slice));
    line.push(0);
    let mut application = program;
    application.push(0);
    let environment = Environment::of_user(shell)?;
    let parent: HANDLE = shell.process().as_raw_handle();
    let mut list = AttributeList::new()?;
    list.set_parent(&parent)?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb =
        u32::try_from(size_of::<STARTUPINFOEXW>()).map_err(io::Error::other)?;
    startup.lpAttributeList = list.as_raw();
    let mut info = PROCESS_INFORMATION::default();
    // SAFETY: `application`, `line` and `directory` are NUL-terminated UTF-16 that live for the
    // call (`line` is writable, as CreateProcessW requires); `environment` is a live Unicode block,
    // as the flag says; `startup` is a live STARTUPINFOEXW whose attribute list and parent handle
    // outlive the call; `info` is a live, writable out-parameter.
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            line.as_mut_ptr(),
            null(),
            null(),
            0,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
            environment.block,
            directory.as_ptr(),
            (&raw const startup).cast(),
            &raw mut info,
        )
    };
    if created == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: CreateProcessW succeeded, so both handles are new and owned by nothing else.
    let (process, _thread) = unsafe {
        (
            OwnedHandle::from_raw_handle(info.hProcess),
            OwnedHandle::from_raw_handle(info.hThread),
        )
    };
    Ok(ShellChild { process })
}

impl ShellChild {
    /// Waits up to `millis` (`INFINITE` for no deadline); `None` means the child still runs.
    fn wait_ms(&self, millis: u32) -> io::Result<Option<i32>> {
        // SAFETY: the process handle is live while `self` is.
        let waited = unsafe { WaitForSingleObject(self.process.as_raw_handle(), millis) };
        match waited {
            WAIT_OBJECT_0 => self.exit_code().map(Some),
            WAIT_TIMEOUT => Ok(None),
            WAIT_FAILED => Err(io::Error::last_os_error()),
            other => Err(io::Error::other(format!(
                "unexpected wait result {other:#x}"
            ))),
        }
    }

    /// Reads the exited child's code; codes above `i32::MAX` wrap, as `std::process` reports them.
    fn exit_code(&self) -> io::Result<i32> {
        let mut code = 0u32;
        // SAFETY: the process handle is live and `code` is a writable u32.
        if unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &raw mut code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(i32::from_ne_bytes(code.to_ne_bytes()))
    }
}

impl Child for ShellChild {
    /// Waits up to `timeout` (capped just below `INFINITE`); `None` means the child still runs.
    fn wait_timeout(&mut self, timeout: Duration) -> io::Result<Option<i32>> {
        let millis = u32::try_from(timeout.as_millis())
            .unwrap_or(u32::MAX)
            .min(INFINITE - 1);
        self.wait_ms(millis)
    }

    /// Waits without a deadline and returns the exit code.
    fn wait(&mut self) -> io::Result<i32> {
        self.wait_ms(INFINITE)?
            .ok_or_else(|| io::Error::other("an INFINITE wait timed out"))
    }
}

/// The shell user's own environment block (Unicode), made without Setup's variables; dropping it
/// destroys the block on every path.
struct Environment {
    block: *mut c_void,
}

impl Environment {
    /// Builds the block from the shell user's token. Fails with the Windows error.
    fn of_user(shell: &Shell) -> io::Result<Self> {
        let mut block = null_mut();
        // SAFETY: `block` is a writable pointer slot; the token handle is live and has TOKEN_QUERY,
        // which an impersonation token needs; FALSE leaves Setup's environment out.
        let made =
            unsafe { CreateEnvironmentBlock(&raw mut block, shell.token().as_raw_handle(), 0) };
        if made == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { block })
    }
}

impl Drop for Environment {
    /// Releases the block CreateEnvironmentBlock made.
    fn drop(&mut self) {
        // SAFETY: the block came from CreateEnvironmentBlock and is destroyed only here.
        unsafe { DestroyEnvironmentBlock(self.block) };
    }
}

/// A `PROC_THREAD_ATTRIBUTE_LIST` with room for one attribute, in a `usize`-aligned buffer; `'a`
/// keeps the parent handle alive, because Windows stores its address, not a copy.
struct AttributeList<'a> {
    buffer: Vec<usize>,
    parent: PhantomData<&'a HANDLE>,
}

impl<'a> AttributeList<'a> {
    /// Allocates and initialises the list. Fails when Windows cannot size or initialise it.
    fn new() -> io::Result<Self> {
        let mut bytes = 0;
        // SAFETY: a null list asks only for the size; Windows writes `bytes` and fails by design.
        unsafe { InitializeProcThreadAttributeList(null_mut(), 1, 0, &raw mut bytes) };
        if bytes == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut buffer = vec![0usize; bytes.div_ceil(size_of::<usize>())];
        // SAFETY: `buffer` is live, writable and at least `bytes` long.
        let ok = unsafe {
            InitializeProcThreadAttributeList(buffer.as_mut_ptr().cast(), 1, 0, &raw mut bytes)
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            buffer,
            parent: PhantomData,
        })
    }

    /// Sets `PROC_THREAD_ATTRIBUTE_PARENT_PROCESS` to `parent`, a handle with
    /// `PROCESS_CREATE_PROCESS`. Fails when Windows refuses the attribute.
    fn set_parent(&mut self, parent: &'a HANDLE) -> io::Result<()> {
        // SAFETY: the list is initialised; `parent` outlives the list through `'a`.
        let ok = unsafe {
            UpdateProcThreadAttribute(
                self.as_raw(),
                0,
                PROC_THREAD_ATTRIBUTE_PARENT_PROCESS as usize,
                std::ptr::from_ref(parent).cast(),
                size_of::<HANDLE>(),
                null_mut(),
                null(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// The list pointer for `STARTUPINFOEXW` and the attribute calls.
    fn as_raw(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.buffer.as_mut_ptr().cast()
    }
}

impl Drop for AttributeList<'_> {
    /// Releases what Windows initialised in the list; the buffer frees itself afterwards.
    fn drop(&mut self) {
        // SAFETY: the list was initialised in `new` (a failed init never builds `Self`).
        unsafe { DeleteProcThreadAttributeList(self.as_raw()) };
    }
}

/// Encodes `text` as UTF-16 without a terminator; an embedded NUL is `InvalidInput`, because
/// Windows would cut the string there.
fn wide(text: &OsStr) -> io::Result<Vec<u16>> {
    let units: Vec<u16> = text.encode_wide().collect();
    if units.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a program path, argument or folder contains a NUL",
        ));
    }
    Ok(units)
}

/// Encodes `path` like `wide`, refusing a path that is not absolute (`InvalidInput`), so nothing
/// resolves against Setup's own folder.
fn absolute_wide(path: &OsStr) -> io::Result<Vec<u16>> {
    let units = wide(path)?;
    if !is_absolute(&units) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a program path or folder is not absolute",
        ));
    }
    Ok(units)
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::io;
    use std::path::PathBuf;
    use std::time::Duration;

    use super::{absolute_wide, spawn_as_shell, wide};
    use crate::probe::shell::{Found, Shell};
    use crate::runner::Child;

    #[test]
    fn wide_refuses_an_embedded_nul() {
        let error = wide(OsStr::new("a\0b")).err().map(|error| error.kind());
        assert_eq!(error, Some(io::ErrorKind::InvalidInput));
        assert_eq!(wide(OsStr::new("ab")).ok(), Some(vec![97, 98]));
    }

    #[test]
    fn absolute_wide_refuses_relative_and_rooted_paths() {
        for path in ["cmd.exe", r"C:cmd.exe", r"\Windows\cmd.exe"] {
            let error = absolute_wide(OsStr::new(path))
                .err()
                .map(|error| error.kind());
            assert_eq!(error, Some(io::ErrorKind::InvalidInput), "{path}");
        }
        assert!(absolute_wide(OsStr::new(r"C:\Windows\cmd.exe")).is_ok());
    }

    #[test]
    #[ignore = "needs a desktop shell; runs in the VM pass (M2)"]
    fn child_of_the_shell_reports_its_exit_code() {
        let shell = match Shell::find() {
            Ok(Found::Shell(shell)) => shell,
            other => {
                assert!(
                    matches!(other, Ok(Found::Shell(_))),
                    "no desktop shell: {other:?}"
                );
                return;
            }
        };
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| OsString::from(r"C:\Windows"));
        let system = PathBuf::from(root).join("System32");
        let cmd = system.join("cmd.exe");
        let args = [OsString::from("/c"), OsString::from("exit 7")];
        let child = spawn_as_shell(&shell, cmd.as_os_str(), &args, &system);
        assert!(child.is_ok(), "{child:?}");
        if let Ok(mut child) = child {
            let waited = child.wait_timeout(Duration::from_secs(30));
            assert_eq!(waited.ok(), Some(Some(7)));
        }
    }
}
