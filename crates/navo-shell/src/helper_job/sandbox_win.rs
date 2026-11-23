//! Hands the PDF helper's sandbox flags to Windows: UI limits on its Job and the mitigation policy
//! in the spawn's attribute list (S2b; the numbers live in `sandbox_policy.rs`).

use std::io;
use std::mem::size_of;
use std::os::windows::io::{AsRawHandle, BorrowedHandle};
use std::ptr;

use windows_sys::Win32::System::JobObjects::{
    JOBOBJECT_BASIC_UI_RESTRICTIONS, JobObjectBasicUIRestrictions, SetInformationJobObject,
};
use windows_sys::Win32::System::Threading::{
    PROC_THREAD_ATTRIBUTE_MITIGATION_POLICY, UpdateProcThreadAttribute,
};

use super::sandbox_policy::{MITIGATIONS, UI_LIMITS};
use super::spawn_win::AttributeList;

const UI_BYTES: u32 = size_of::<JOBOBJECT_BASIC_UI_RESTRICTIONS>() as u32;

/// The mitigation value Windows reads during `CreateProcessW`; a static, because the attribute
/// list stores a pointer to it, not a copy.
static MITIGATION_VALUE: u64 = MITIGATIONS;

/// Sets the helper's UI limits on `job`. Fails when Windows refuses them.
pub(super) fn restrict_ui(job: BorrowedHandle<'_>) -> io::Result<()> {
    let info = JOBOBJECT_BASIC_UI_RESTRICTIONS {
        UIRestrictionsClass: UI_LIMITS,
    };
    // SAFETY: the job handle is live and `info` is a live JOBOBJECT_BASIC_UI_RESTRICTIONS of UI_BYTES bytes.
    let set = unsafe {
        SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectBasicUIRestrictions,
            (&raw const info).cast(),
            UI_BYTES,
        )
    };
    check(set)
}

/// Reads the UI limits set on `job` (`JOB_OBJECT_UILIMIT_*` bits). Fails when the query fails.
#[cfg(test)]
pub(super) fn ui_limits(job: BorrowedHandle<'_>) -> io::Result<u32> {
    use windows_sys::Win32::System::JobObjects::QueryInformationJobObject;
    let mut info = JOBOBJECT_BASIC_UI_RESTRICTIONS {
        UIRestrictionsClass: 0,
    };
    // SAFETY: the job handle is live and `info` is a live, writable buffer of UI_BYTES bytes; the length out-pointer may be null.
    let queried = unsafe {
        QueryInformationJobObject(
            job.as_raw_handle(),
            JobObjectBasicUIRestrictions,
            (&raw mut info).cast(),
            UI_BYTES,
            ptr::null_mut(),
        )
    };
    check(queried)?;
    Ok(info.UIRestrictionsClass)
}

impl AttributeList<'_> {
    /// Sets `PROC_THREAD_ATTRIBUTE_MITIGATION_POLICY` to the helper's mitigations (a 64-bit
    /// value). Fails when Windows refuses the attribute.
    pub(super) fn set_mitigation_policy(&mut self) -> io::Result<()> {
        // SAFETY: the list is initialised; the value is a static, so it outlives the list.
        check(unsafe {
            UpdateProcThreadAttribute(
                self.as_raw(),
                0,
                PROC_THREAD_ATTRIBUTE_MITIGATION_POLICY as usize,
                (&raw const MITIGATION_VALUE).cast(),
                size_of::<u64>(),
                ptr::null_mut(),
                ptr::null(),
            )
        })
    }
}

/// Turns a Win32 `BOOL` result into `Ok`, or the last OS error when it is zero.
fn check(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
