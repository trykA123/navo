//! Puts the PDF helper in its AppContainer at spawn (S2c2): grants the helper exe and its
//! pdfium.dll to the container SID, then adds `SECURITY_CAPABILITIES` (zero capabilities) to the
//! spawn's attribute list. Every failure is a `ContainerError` and no process is created.

use std::io;
use std::mem::size_of;
use std::path::Path;
use std::ptr;

use windows_sys::Win32::Security::SECURITY_CAPABILITIES;
use windows_sys::Win32::System::Threading::{
    PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, UpdateProcThreadAttribute,
};

use super::appcontainer_policy::{GrantError, grant_helper_files};
use super::appcontainer_win::{ContainerSid, grant_read_execute};
use super::container_error::ContainerError;
use super::spawn_win::AttributeList;

/// The helper's AppContainer: its SID and the capabilities value Windows reads during
/// `CreateProcessW` (the attribute list stores a pointer to it, so it must outlive the list).
pub(super) struct Container {
    sid: ContainerSid,
    capabilities: SECURITY_CAPABILITIES,
}

impl Container {
    /// Derives the container SID and grants it read+execute on `program` and the `pdfium.dll`
    /// beside it. Fails closed with the step that failed; the diagnostics log names it too.
    pub(super) fn prepare(program: &Path) -> Result<Self, ContainerError> {
        let sid = ContainerSid::derive().map_err(|error| refused(error, None))?;
        grant_helper_files(program, |target| grant_read_execute(target, &sid))
            .map_err(|error| refused(error, Some(&sid)))?;
        let capabilities = SECURITY_CAPABILITIES {
            AppContainerSid: sid.as_psid(),
            Capabilities: ptr::null_mut(),
            CapabilityCount: 0,
            Reserved: 0,
        };
        Ok(Self { sid, capabilities })
    }
}

impl<'a> AttributeList<'a> {
    /// Sets `PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES`, so the child starts inside `container`
    /// with no capabilities. Fails with a permanent `ContainerError` when Windows refuses it.
    pub(super) fn set_security_capabilities(
        &mut self,
        container: &'a Container,
    ) -> Result<(), ContainerError> {
        // SAFETY: the list is initialised; `container` (its capabilities and the SID they point
        // at) outlives the list through `'a`, because Windows keeps the pointer, not a copy.
        let ok = unsafe {
            UpdateProcThreadAttribute(
                self.as_raw(),
                0,
                PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
                (&raw const container.capabilities).cast(),
                size_of::<SECURITY_CAPABILITIES>(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        if ok == 0 {
            let code = io::Error::last_os_error().raw_os_error().unwrap_or(-1);
            let error = ContainerError::attribute(code);
            tracing::warn!(%error, sid = %sid_text(&container.sid), "the PDF helper was not started");
            return Err(error);
        }
        Ok(())
    }
}

/// Logs a failed grant step with the container SID (for the ACL fix) and wraps it.
fn refused(error: GrantError, sid: Option<&ContainerSid>) -> ContainerError {
    let error = ContainerError::grant(error);
    let sid = sid.map_or_else(|| "unknown".to_owned(), sid_text);
    tracing::warn!(%error, %sid, transient = error.is_transient(), "the PDF helper was not started");
    error
}

/// The SID as `S-1-15-2-…`, or the reason it cannot be formatted.
fn sid_text(sid: &ContainerSid) -> String {
    sid.to_sddl().unwrap_or_else(|error| error.to_string())
}
