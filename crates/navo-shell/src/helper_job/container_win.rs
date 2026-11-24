//! Puts the PDF helper in its AppContainer at spawn (S2c2): registers the container's profile,
//! grants the helper exe and its pdfium.dll to the container SID, then adds
//! `SECURITY_CAPABILITIES` (zero capabilities) to the spawn's attribute list. Every failure is a
//! `ContainerError` and no process is created. Uninstall removes the profile again (U3a).

use std::io;
use std::mem::size_of;
use std::path::Path;
use std::ptr;

use windows_sys::Win32::Security::Isolation::{
    CreateAppContainerProfile, DeleteAppContainerProfile,
};
use windows_sys::Win32::Security::{FreeSid, PSID, SECURITY_CAPABILITIES};
use windows_sys::Win32::System::Threading::{
    PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, UpdateProcThreadAttribute,
};

use super::appcontainer_policy::{CONTAINER_NAME, GrantError, GrantStep, grant_helper_files};
use super::appcontainer_win::{ContainerSid, grant_read_execute};
use super::container_error::{ContainerError, profile_removal};
use super::spawn_win::AttributeList;

/// The helper's AppContainer: its SID and the capabilities value Windows reads during
/// `CreateProcessW` (the attribute list stores a pointer to it, so it must outlive the list).
pub(super) struct Container {
    sid: ContainerSid,
    capabilities: SECURITY_CAPABILITIES,
}

impl Container {
    /// Registers the container's profile, derives its SID and grants it read+execute on `program`
    /// and the `pdfium.dll` beside it. Fails closed with the step that failed; the diagnostics log
    /// names it too.
    pub(super) fn prepare(program: &Path) -> Result<Self, ContainerError> {
        register_profile().map_err(|error| refused(error, None))?;
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

/// `HRESULT_FROM_WIN32(ERROR_ALREADY_EXISTS)`: the profile is registered already.
const PROFILE_EXISTS: i32 = 0x8007_00B7_u32 as i32;

/// Registers the `CONTAINER_NAME` AppContainer profile (zero capabilities) unless it exists.
/// `CreateProcessW` looks the container up there and fails with ERROR_FILE_NOT_FOUND without it.
/// Fails at `CreateProfile` with the HRESULT.
fn register_profile() -> Result<(), GrantError> {
    let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (name, display) = (wide(CONTAINER_NAME), wide("Navo PDF helper"));
    let mut sid: PSID = ptr::null_mut();
    // SAFETY: the strings are NUL-terminated and live for the call; zero capabilities need no
    // array; `sid` is a live out-pointer.
    let hr = unsafe {
        CreateAppContainerProfile(
            name.as_ptr(),
            display.as_ptr(),
            display.as_ptr(),
            ptr::null(),
            0,
            &raw mut sid,
        )
    };
    if !sid.is_null() {
        // SAFETY: CreateAppContainerProfile returned this SID and says to free it with FreeSid.
        unsafe { FreeSid(sid) };
    }
    match hr {
        0.. | PROFILE_EXISTS => Ok(()),
        _ => Err(GrantError::new(GrantStep::CreateProfile, None, hr)),
    }
}

/// Deletes the `CONTAINER_NAME` AppContainer profile (uninstall). `Ok` when no profile is left,
/// also when none existed; fails with the HRESULT of any other answer.
pub fn remove_pdf_container_profile() -> Result<(), i32> {
    let name: Vec<u16> = CONTAINER_NAME.encode_utf16().chain([0]).collect();
    // SAFETY: `name` is NUL-terminated and lives for the call.
    let hr = unsafe { DeleteAppContainerProfile(name.as_ptr()) };
    profile_removal(hr)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_registered_profile_is_removed_twice_in_a_row() {
        register_profile().unwrap();
        assert_eq!(remove_pdf_container_profile(), Ok(()));
        let name: Vec<u16> = CONTAINER_NAME.encode_utf16().chain([0]).collect();
        // SAFETY: `name` is NUL-terminated and lives for the call.
        let hr = unsafe { DeleteAppContainerProfile(name.as_ptr()) };
        eprintln!(
            "second DeleteAppContainerProfile HRESULT: {:#010x}",
            hr as u32
        );
        assert_eq!(profile_removal(hr), Ok(()), "{:#010x}", hr as u32);
    }
}
