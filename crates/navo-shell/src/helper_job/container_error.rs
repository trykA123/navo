//! Why the PDF helper's AppContainer could not be set up at spawn (S2c2): the failed step, whether
//! a retry may help, and the `io::Error` `spawn_confined` returns for it. `container_win.rs` raises it.

use std::fmt;
use std::io;

use super::appcontainer_policy::{ErrorKind, GrantError, GrantStep};

/// Win32 ERROR_FILE_NOT_FOUND and ERROR_PATH_NOT_FOUND.
const FILE_NOT_FOUND: i32 = 2;
const PATH_NOT_FOUND: i32 = 3;

/// A helper file the grant could not open because it does not exist: no install problem with
/// confinement, so the caller reports the engine or the helper as missing, not as unsafe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingFile {
    /// `pdfium.dll` beside the helper exe.
    Engine,
    /// The helper exe itself.
    Helper,
}

/// A failed AppContainer setup: `spawn_confined` started no process. Reach it from the returned
/// `io::Error` with `get_ref()` and `downcast_ref::<ContainerError>()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerError(Cause);

/// The step that failed, with its Win32 error or HRESULT.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Cause {
    /// Deriving the SID or granting the helper's files (S2c1 policy).
    Grant(GrantError),
    /// `UpdateProcThreadAttribute` refused `SECURITY_CAPABILITIES`.
    Attribute(i32),
}

impl ContainerError {
    /// Wraps a failed SID derivation or file grant.
    pub(super) fn grant(error: GrantError) -> Self {
        Self(Cause::Grant(error))
    }

    /// Wraps a refused `SECURITY_CAPABILITIES` attribute with its Win32 error `code`.
    pub(super) fn attribute(code: i32) -> Self {
        Self(Cause::Attribute(code))
    }

    /// True when a retry may succeed (a lock or low memory during the grant). A refused attribute
    /// is permanent: the system cannot start AppContainer processes.
    pub fn is_transient(&self) -> bool {
        match &self.0 {
            Cause::Grant(error) => error.kind() == ErrorKind::Transient,
            Cause::Attribute(_) => false,
        }
    }

    /// Which helper file was missing when the grant's OpenFile step failed with "file or path not
    /// found"; `None` for every other failure.
    pub fn missing_file(&self) -> Option<MissingFile> {
        let Cause::Grant(error) = &self.0 else {
            return None;
        };
        if error.step != GrantStep::OpenFile
            || !matches!(error.code, FILE_NOT_FOUND | PATH_NOT_FOUND)
        {
            return None;
        }
        let name = error.target.as_deref().and_then(std::path::Path::file_name);
        let is_engine = name.is_some_and(|name| name.eq_ignore_ascii_case("pdfium.dll"));
        Some(if is_engine {
            MissingFile::Engine
        } else {
            MissingFile::Helper
        })
    }

    /// The Win32 error or HRESULT of the failed step.
    fn code(&self) -> i32 {
        match &self.0 {
            Cause::Grant(error) => error.code,
            Cause::Attribute(code) => *code,
        }
    }
}

impl fmt::Display for ContainerError {
    /// Names the failed step: the grant's own message, or the attribute step and its error.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Cause::Grant(error) => error.fmt(f),
            Cause::Attribute(code) => write!(
                f,
                "AppContainer setup failed at SecurityCapabilities: OS error {code:#x} (Permanent)"
            ),
        }
    }
}

/// HRESULTs `DeleteAppContainerProfile` returns when no profile is left: `S_OK`,
/// `HRESULT_FROM_WIN32(ERROR_NOT_FOUND)` and `HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)`.
/// A second delete of a removed profile returns S_OK (Windows runner, 2026-11-24).
const PROFILE_GONE: [i32; 3] = [0, 0x8007_0490_u32 as i32, 0x8007_0002_u32 as i32];

/// Maps the HRESULT of `DeleteAppContainerProfile` to the removal result: `Ok` when nothing is
/// left, `Err(hr)` for any other code (access denied, a locked profile).
pub(super) fn profile_removal(hr: i32) -> Result<(), i32> {
    if PROFILE_GONE.contains(&hr) {
        Ok(())
    } else {
        Err(hr)
    }
}

impl std::error::Error for ContainerError {}

impl From<ContainerError> for io::Error {
    /// Keeps the OS error's kind (a missing exe stays `NotFound`) and the typed error inside.
    fn from(error: ContainerError) -> Self {
        io::Error::new(io::Error::from_raw_os_error(error.code()).kind(), error)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::appcontainer_policy::GrantStep;
    use super::*;

    fn grant(step: GrantStep, code: i32) -> ContainerError {
        ContainerError::grant(GrantError::new(step, Some(Path::new("pdfium.dll")), code))
    }

    #[test]
    fn a_lock_during_the_grant_is_transient_and_denial_is_not() {
        assert!(grant(GrantStep::OpenFile, 32).is_transient());
        assert!(!grant(GrantStep::WriteDacl, 5).is_transient());
        assert!(!grant(GrantStep::DeriveSid, 0x8007_0005_u32 as i32).is_transient());
    }

    #[test]
    fn a_missing_helper_file_is_named_not_refused() {
        let at = |step, file: &str, code| {
            ContainerError::grant(GrantError::new(step, Some(Path::new(file)), code))
        };
        let dll = "C:/Navo/pdfium.dll";
        let exe = "C:/Navo/navo.exe";
        assert_eq!(
            at(GrantStep::OpenFile, dll, 2).missing_file(),
            Some(MissingFile::Engine)
        );
        assert_eq!(
            at(GrantStep::OpenFile, "PDFIUM.DLL", 3).missing_file(),
            Some(MissingFile::Engine)
        );
        assert_eq!(
            at(GrantStep::OpenFile, exe, 2).missing_file(),
            Some(MissingFile::Helper)
        );
        assert_eq!(at(GrantStep::OpenFile, dll, 5).missing_file(), None);
        assert_eq!(at(GrantStep::ReadDacl, dll, 2).missing_file(), None);
        assert_eq!(ContainerError::attribute(2).missing_file(), None);
    }

    #[test]
    fn a_refused_attribute_is_permanent_and_named() {
        let error = ContainerError::attribute(87);
        assert!(!error.is_transient());
        assert!(
            error.to_string().contains("SecurityCapabilities"),
            "{error}"
        );
    }

    #[test]
    fn the_io_error_keeps_the_os_kind_and_the_typed_error() {
        let typed = grant(GrantStep::OpenFile, 2);
        let error = io::Error::from(typed.clone());
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        let inner = error
            .get_ref()
            .and_then(|e| e.downcast_ref::<ContainerError>());
        assert_eq!(inner, Some(&typed));
        assert!(error.to_string().contains("OpenFile"), "{error}");
        assert!(error.to_string().contains("pdfium.dll"), "{error}");
    }

    #[test]
    fn a_removed_or_missing_profile_is_ok_and_other_codes_are_errors() {
        assert_eq!(profile_removal(0), Ok(()));
        assert_eq!(profile_removal(0x8007_0490_u32 as i32), Ok(()));
        assert_eq!(profile_removal(0x8007_0002_u32 as i32), Ok(()));
        for hr in [
            1,
            0x8007_0057_u32 as i32,
            0x8007_0005_u32 as i32,
            0x8007_0020_u32 as i32,
            0x8000_4005_u32 as i32,
        ] {
            assert_eq!(profile_removal(hr), Err(hr), "{hr:#x}");
        }
    }
}
