//! Starts the PDF helper suspended inside its AppContainer, puts it in its Job, then lets it run,
//! so it never executes unconfined; only its two pipe ends are inherited (S2a1, S2c2; `navo-app`
//! plugs it into navo-pdf).

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::marker::PhantomData;
use std::mem::{size_of, size_of_val};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::ptr;

use windows_sys::Win32::Foundation::{
    HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION, ResumeThread, STARTF_USESTDHANDLES,
    STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject,
};

use super::HelperLimits;
use super::cmdline::command_line;
use super::container_win::Container;
use super::platform::HelperJob;

/// A helper started by `spawn_confined`: the parent's pipe ends and the confined process.
pub struct ConfinedChild {
    pub stdin: File,
    pub stdout: File,
    pub process: ConfinedProcess,
}

/// The helper process handle (full access) and the Job it runs in; dropping it closes the Job,
/// which kills the process.
pub struct ConfinedProcess {
    handle: OwnedHandle,
    job: HelperJob,
}

/// Starts `program` with `args` suspended, windowless and inside the helper's AppContainer, assigns
/// it to a new Job with `limits`, then resumes it. Fails on any step; a failed grant or attribute
/// fails closed with a `ContainerError` inside the `io::Error` and creates no process. After
/// `CreateProcessW` a failure terminates the process first, so an `Err` leaves no live process.
pub fn spawn_confined(
    program: &Path,
    args: &[OsString],
    limits: HelperLimits,
) -> io::Result<ConfinedChild> {
    let mut application = wide(program.as_os_str())?;
    let args = args
        .iter()
        .map(|arg| wide(arg.as_os_str()))
        .collect::<io::Result<Vec<_>>>()?;
    let mut line = command_line(&application, args.iter().map(Vec::as_slice));
    line.push(0);
    application.push(0);
    let container = Container::prepare(program)?;
    let job = HelperJob::create(limits)?;
    let (child_in, parent_in) = io::pipe()?;
    let (parent_out, child_out) = io::pipe()?;
    let child_in = inheritable(child_in.into())?;
    let child_out = inheritable(child_out.into())?;
    let setup = ChildSetup {
        inherit: [child_in.as_raw_handle(), child_out.as_raw_handle()],
        container: &container,
    };
    let (handle, thread) = create_suspended(&application, &mut line, &setup)?;
    // The child holds its own copies now; ours must close, or its stdout never reaches EOF.
    drop((child_in, child_out));
    let process = ConfinedProcess { handle, job };
    if let Err(error) = process.start(&thread) {
        let _ = process.terminate();
        return Err(error);
    }
    Ok(ConfinedChild {
        stdin: File::from(OwnedHandle::from(parent_in)),
        stdout: File::from(OwnedHandle::from(parent_out)),
        process,
    })
}

impl ConfinedChild {
    /// Ends the process; `Ok` when it has already exited. See `ConfinedProcess::kill`.
    pub fn kill(&self) -> io::Result<()> {
        self.process.kill()
    }

    /// Checks without blocking whether the process has exited (`true` = exited).
    pub fn try_wait(&self) -> io::Result<bool> {
        self.process.try_wait()
    }

    /// The `CreateProcessW` process handle, with full access.
    #[must_use]
    pub fn process_handle(&self) -> BorrowedHandle<'_> {
        self.process.process_handle()
    }
}

impl ConfinedProcess {
    /// Ends the process with exit code 1. Returns `Ok` when it has already exited; otherwise the
    /// `TerminateProcess` error.
    pub fn kill(&self) -> io::Result<()> {
        match self.terminate() {
            Ok(()) => Ok(()),
            Err(_) if self.try_wait()? => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// Checks without blocking whether the process has exited (`true` = exited). Fails when the
    /// wait itself fails.
    pub fn try_wait(&self) -> io::Result<bool> {
        // SAFETY: the process handle is live while `self` is; a zero timeout only polls.
        match unsafe { WaitForSingleObject(self.handle.as_raw_handle(), 0) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(io::Error::last_os_error()),
        }
    }

    /// The `CreateProcessW` process handle, with full access.
    #[must_use]
    pub fn process_handle(&self) -> BorrowedHandle<'_> {
        self.handle.as_handle()
    }

    /// The Job the process runs in.
    #[must_use]
    pub fn job(&self) -> &HelperJob {
        &self.job
    }

    /// Assigns the suspended process to its Job, then resumes its first thread. Fails on either.
    fn start(&self, thread: &OwnedHandle) -> io::Result<()> {
        self.job.assign(&self.handle)?;
        // SAFETY: `thread` is the live first-thread handle `CreateProcessW` returned.
        if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Calls `TerminateProcess` with exit code 1. Fails with its error.
    fn terminate(&self) -> io::Result<()> {
        // SAFETY: the process handle is live while `self` is and has PROCESS_TERMINATE.
        check(unsafe { TerminateProcess(self.handle.as_raw_handle(), 1) })
    }
}

/// What the child starts with besides its command line: the stdin and stdout handles (its only
/// inherited ones) and its AppContainer.
struct ChildSetup<'a> {
    inherit: [HANDLE; 2],
    container: &'a Container,
}

/// Calls `CreateProcessW` suspended and windowless, with `setup.inherit` as stdin and stdout and
/// as the only inherited handles, the S2b mitigation policy and the S2c2 AppContainer. Returns the
/// process and thread handles; a refused attribute fails before any process exists.
fn create_suspended(
    application: &[u16],
    line: &mut [u16],
    setup: &ChildSetup<'_>,
) -> io::Result<(OwnedHandle, OwnedHandle)> {
    let mut list = AttributeList::new(3)?;
    list.set_handle_list(&setup.inherit)?;
    list.set_mitigation_policy()?;
    list.set_security_capabilities(setup.container)?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = setup.inherit[0];
    startup.StartupInfo.hStdOutput = setup.inherit[1];
    startup.lpAttributeList = list.as_raw();
    let mut info = PROCESS_INFORMATION::default();
    let flags = CREATE_SUSPENDED | CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT;
    // SAFETY: `application` and `line` are NUL-terminated UTF-16 that live for the call (`line` is
    // writable, as CreateProcessW requires); `startup` is a live STARTUPINFOEXW whose attribute
    // list and inherited handles outlive the call; `info` is a live, writable out-parameter.
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            line.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            1,
            flags,
            ptr::null(),
            ptr::null(),
            (&raw const startup).cast(),
            &raw mut info,
        )
    };
    check(created)?;
    // SAFETY: CreateProcessW succeeded, so both handles are new and owned by nothing else.
    Ok(unsafe {
        (
            OwnedHandle::from_raw_handle(info.hProcess),
            OwnedHandle::from_raw_handle(info.hThread),
        )
    })
}

/// A `PROC_THREAD_ATTRIBUTE_LIST` in a `usize`-aligned buffer; `'a` keeps the values it points
/// at alive, because Windows stores the pointers, not copies.
pub(super) struct AttributeList<'a> {
    buffer: Vec<usize>,
    values: PhantomData<&'a [HANDLE]>,
}

impl<'a> AttributeList<'a> {
    /// Allocates and initialises a list with room for `count` attributes. Fails when Windows
    /// cannot size or initialise it.
    fn new(count: u32) -> io::Result<Self> {
        let mut bytes = 0;
        // SAFETY: a null list asks only for the size; Windows writes `bytes` and fails by design.
        unsafe { InitializeProcThreadAttributeList(ptr::null_mut(), count, 0, &raw mut bytes) };
        if bytes == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut buffer = vec![0usize; bytes.div_ceil(size_of::<usize>())];
        // SAFETY: `buffer` is live, writable and at least `bytes` long.
        let ok = unsafe {
            InitializeProcThreadAttributeList(buffer.as_mut_ptr().cast(), count, 0, &raw mut bytes)
        };
        check(ok)?;
        Ok(Self {
            buffer,
            values: PhantomData,
        })
    }

    /// Sets `PROC_THREAD_ATTRIBUTE_HANDLE_LIST`: the child inherits only `handles`, which must be
    /// inheritable. Fails when Windows refuses the attribute.
    fn set_handle_list(&mut self, handles: &'a [HANDLE]) -> io::Result<()> {
        // SAFETY: the list is initialised; `handles` outlives the list through `'a`.
        check(unsafe {
            UpdateProcThreadAttribute(
                self.as_raw(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast(),
                size_of_val(handles),
                ptr::null_mut(),
                ptr::null(),
            )
        })
    }

    /// The list pointer for `STARTUPINFOEXW` and the attribute calls.
    pub(super) fn as_raw(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
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

/// Marks `handle` inheritable, as the handle list requires. Fails when Windows refuses it.
fn inheritable(handle: OwnedHandle) -> io::Result<OwnedHandle> {
    // SAFETY: `handle` is live and owned here; only its inherit flag changes.
    let ok = unsafe {
        SetHandleInformation(
            handle.as_raw_handle(),
            HANDLE_FLAG_INHERIT,
            HANDLE_FLAG_INHERIT,
        )
    };
    check(ok)?;
    Ok(handle)
}

/// Encodes `text` as UTF-16 without a terminator. Refuses an embedded NUL with `InvalidInput`,
/// because Windows would cut the string there.
fn wide(text: &OsStr) -> io::Result<Vec<u16>> {
    let units: Vec<u16> = text.encode_wide().collect();
    if units.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a helper path or argument contains a NUL",
        ));
    }
    Ok(units)
}

/// Turns a Win32 `BOOL` result into `Ok`, or the last OS error when it is zero.
fn check(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
