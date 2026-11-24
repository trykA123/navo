//! Why the PDF helper's AppContainer could not be set up at spawn (S2c2): the failed step, whether
//! a retry may help, and the `io::Error` `spawn_confined` returns for it. `container_win.rs` raises it.

use std::fmt;
use std::io;

use super::appcontainer_policy::{ErrorKind, GrantError};

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
}
