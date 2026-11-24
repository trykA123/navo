//! Starts a child as the desktop user: the shell process is its parent, so the child runs with the
//! shell user's token in the shell's session (IW34, H4). The child inherits no handles.

#![cfg(windows)]
#![allow(
    unsafe_code,
    reason = "the parent-process attribute and the process wait require Win32 FFI"
)]

use std::ffi::{OsStr, OsString};
use std::io;
use std::marker::PhantomData;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::ptr::{null, null_mut};
use std::time::Duration;

use windows_sys::Win32::Foundation::{HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT,
    GetExitCodeProcess, INFINITE, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROC_THREAD_ATTRIBUTE_PARENT_PROCESS, PROCESS_INFORMATION, STARTUPINFOEXW,
    UpdateProcThreadAttribute, WaitForSingleObject,
};

use super::shell::Shell;
use crate::cmdline::command_line;
use crate::runner::Child;

/// A child started by `spawn_as_shell`. Dropping it closes the handle and never kills the child.
#[derive(Debug)]
pub struct ShellChild {
    process: OwnedHandle,
}

/// Starts `program` with `args` as the shell user, with the shell as its parent process.
/// No handles are inherited and the child gets Setup's environment; `cwd` `None` keeps Setup's.
/// Fails with the Windows error, or `InvalidInput` when a path or argument holds a NUL.
pub fn spawn_as_shell(
    shell: &Shell,
    program: &OsStr,
    args: &[OsString],
    cwd: Option<&Path>,
) -> io::Result<ShellChild> {
    let program = wide(program)?;
    let units = args
        .iter()
        .map(|arg| wide(arg.as_os_str()))
        .collect::<io::Result<Vec<_>>>()?;
    let mut line = command_line(&program, units.iter().map(Vec::as_slice));
    line.push(0);
    let mut application = program;
    application.push(0);
    let directory = cwd.map(|path| wide_z(path.as_os_str())).transpose()?;
    let parent: HANDLE = shell.process().as_raw_handle();
    let mut list = AttributeList::new()?;
    list.set_parent(&parent)?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb =
        u32::try_from(size_of::<STARTUPINFOEXW>()).map_err(io::Error::other)?;
    startup.lpAttributeList = list.as_raw();
    let mut info = PROCESS_INFORMATION::default();
    // SAFETY: `application` and `line` are NUL-terminated UTF-16 that live for the call (`line` is
    // writable, as CreateProcessW requires); `directory` is NUL-terminated or null; `startup` is a
    // live STARTUPINFOEXW whose attribute list and parent handle outlive the call; `info` is a
    // live, writable out-parameter.
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            line.as_mut_ptr(),
            null(),
            null(),
            0,
            EXTENDED_STARTUPINFO_PRESENT,
            null(),
            directory.as_ref().map_or(null(), |path| path.as_ptr()),
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

/// Encodes `text` as NUL-terminated UTF-16, refusing an embedded NUL like `wide`.
fn wide_z(text: &OsStr) -> io::Result<Vec<u16>> {
    let mut units = wide(text)?;
    units.push(0);
    Ok(units)
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::io;
    use std::path::PathBuf;
    use std::time::Duration;

    use super::{spawn_as_shell, wide, wide_z};
    use crate::probe::shell::{Found, Shell};
    use crate::runner::Child;

    #[test]
    fn wide_refuses_an_embedded_nul() {
        let error = wide(OsStr::new("a\0b")).err().map(|error| error.kind());
        assert_eq!(error, Some(io::ErrorKind::InvalidInput));
        assert_eq!(wide_z(OsStr::new("ab")).ok(), Some(vec![97, 98, 0]));
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
        let cmd = PathBuf::from(root).join("System32").join("cmd.exe");
        let args = [OsString::from("/c"), OsString::from("exit 7")];
        let child = spawn_as_shell(&shell, cmd.as_os_str(), &args, None);
        assert!(child.is_ok(), "{child:?}");
        if let Ok(mut child) = child {
            let waited = child.wait_timeout(Duration::from_secs(30));
            assert_eq!(waited.ok(), Some(Some(7)));
        }
    }
}
