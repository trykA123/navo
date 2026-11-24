//! The platform-neutral half of the PDF helper's AppContainer grant (S2c1): which ACE counts as
//! the grant, how a failed step is named, and the fail-closed rule. `appcontainer_win.rs` calls Win32.

use std::fmt;
use std::path::{Path, PathBuf};

/// The fixed AppContainer name; Windows derives the same SID from it on every call.
pub const CONTAINER_NAME: &str = "Navo.PdfHelper";

/// `FILE_GENERIC_READ | FILE_GENERIC_EXECUTE` (0x120089 | 0x1200A0), the only rights granted.
pub const READ_EXECUTE: u32 = 0x0012_00A9;

/// `ACCESS_ALLOWED_ACE_TYPE` and the `INHERITED_ACE` flag (winnt.h).
const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
const INHERITED_ACE: u8 = 0x10;

/// Win32 errors a retry may clear: sharing and lock violations, low memory or system resources.
const SHARING_VIOLATION: u32 = 32;
const LOCK_VIOLATION: u32 = 33;
const NOT_ENOUGH_MEMORY: u32 = 8;
const OUTOFMEMORY: u32 = 14;
const NO_SYSTEM_RESOURCES: u32 = 1450;

/// The step of a grant that failed; the diagnostics log names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantStep {
    /// The caller passed no file to grant; nothing would be checked.
    NoTargets,
    DeriveSid,
    FormatSid,
    OpenFile,
    ReadDacl,
    ScanDacl,
    BuildDacl,
    WriteDacl,
}

/// Whether a retry may succeed (a lock or low memory) or not (denied, missing, bad ACL).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Transient,
    Permanent,
}

/// A failed grant: the step, the file (none for SID steps) and the Win32 error or HRESULT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantError {
    pub step: GrantStep,
    pub target: Option<PathBuf>,
    pub code: i32,
}

/// What a successful grant did: added the ACE, or found it already there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantOutcome {
    Added,
    AlreadyGranted,
}

impl GrantError {
    /// Builds an error for `step` on `target` with the OS `code`.
    pub fn new(step: GrantStep, target: Option<&Path>, code: i32) -> Self {
        Self {
            step,
            target: target.map(Path::to_path_buf),
            code,
        }
    }

    /// Sorts the error: sharing or lock violations and low memory are transient, the rest permanent.
    pub fn kind(&self) -> ErrorKind {
        // An HRESULT of facility Win32 (0x8007xxxx) carries the Win32 error in its low 16 bits.
        let raw = self.code as u32;
        let win32 = if raw & 0xFFFF_0000 == 0x8007_0000 {
            raw & 0xFFFF
        } else {
            raw
        };
        match win32 {
            SHARING_VIOLATION | LOCK_VIOLATION | NOT_ENOUGH_MEMORY | OUTOFMEMORY
            | NO_SYSTEM_RESOURCES => ErrorKind::Transient,
            _ => ErrorKind::Permanent,
        }
    }
}

impl fmt::Display for GrantError {
    /// Writes "AppContainer grant failed at <step> [on <file>]: OS error <code>".
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AppContainer grant failed at {:?}", self.step)?;
        if let Some(target) = &self.target {
            write!(f, " on {}", target.display())?;
        }
        write!(f, ": OS error {:#x} ({:?})", self.code, self.kind())
    }
}

impl std::error::Error for GrantError {}

/// True when an ACE of `ace_type` with `flags` and access `mask` is the grant: an explicit
/// (not inherited) allow ACE that holds every read+execute bit.
pub fn ace_is_grant(ace_type: u8, flags: u8, mask: u32) -> bool {
    ace_type == ACCESS_ALLOWED_ACE_TYPE
        && flags & INHERITED_ACE == 0
        && mask & READ_EXECUTE == READ_EXECUTE
}

/// Grants every target in order with `grant` and fails closed: the first error stops the run
/// and is returned, so the helper must not start. An empty list is refused too.
pub fn grant_all<F>(targets: &[&Path], mut grant: F) -> Result<(), GrantError>
where
    F: FnMut(&Path) -> Result<GrantOutcome, GrantError>,
{
    if targets.is_empty() {
        return Err(GrantError::new(GrantStep::NoTargets, None, 0));
    }
    for target in targets {
        grant(target)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn denied(path: &Path) -> GrantError {
        GrantError::new(GrantStep::WriteDacl, Some(path), 5)
    }

    #[test]
    fn read_execute_is_generic_read_plus_generic_execute() {
        assert_eq!(READ_EXECUTE, 0x0012_0089 | 0x0012_00A0);
    }

    #[test]
    fn only_an_explicit_full_allow_ace_counts_as_the_grant() {
        assert!(ace_is_grant(0, 0, READ_EXECUTE));
        assert!(ace_is_grant(0, 0, READ_EXECUTE | 0x0002));
        assert!(!ace_is_grant(0, INHERITED_ACE, READ_EXECUTE));
        assert!(!ace_is_grant(1, 0, READ_EXECUTE));
        assert!(!ace_is_grant(0, 0, 0x0012_0089));
    }

    #[test]
    fn every_target_is_granted_in_order() {
        let (a, b) = (Path::new("navo.exe"), Path::new("pdfium.dll"));
        let mut seen = Vec::new();
        let result = grant_all(&[a, b], |path| {
            seen.push(path.to_path_buf());
            Ok(GrantOutcome::Added)
        });
        assert_eq!(result, Ok(()));
        assert_eq!(seen, [a, b]);
    }

    #[test]
    fn a_failed_grant_stops_the_run_and_is_returned() {
        let (a, b) = (Path::new("navo.exe"), Path::new("pdfium.dll"));
        let mut calls = 0;
        let result = grant_all(&[a, b], |path| {
            calls += 1;
            Err(denied(path))
        });
        assert_eq!(result, Err(denied(a)));
        assert_eq!(calls, 1);
    }

    #[test]
    fn a_failure_on_the_last_target_still_fails_closed() {
        let (a, b) = (Path::new("navo.exe"), Path::new("pdfium.dll"));
        let result = grant_all(&[a, b], |path| {
            if path == b {
                Err(denied(path))
            } else {
                Ok(GrantOutcome::AlreadyGranted)
            }
        });
        assert_eq!(result, Err(denied(b)));
    }

    #[test]
    fn an_empty_target_list_is_refused() {
        let result = grant_all(&[], |_| Ok(GrantOutcome::Added));
        assert_eq!(result.map_err(|e| e.step), Err(GrantStep::NoTargets));
    }

    #[test]
    fn locks_and_low_memory_are_transient_and_denial_is_permanent() {
        let kind = |code| GrantError::new(GrantStep::OpenFile, None, code).kind();
        assert_eq!(kind(32), ErrorKind::Transient);
        assert_eq!(kind(33), ErrorKind::Transient);
        assert_eq!(kind(14), ErrorKind::Transient);
        // E_OUTOFMEMORY, an HRESULT wrapping Win32 error 14.
        assert_eq!(kind(0x8007_000E_u32 as i32), ErrorKind::Transient);
        assert_eq!(kind(5), ErrorKind::Permanent);
        assert_eq!(kind(2), ErrorKind::Permanent);
        assert_eq!(kind(0x8007_0005_u32 as i32), ErrorKind::Permanent);
    }

    #[test]
    fn the_message_names_the_step_and_the_file() {
        let text = denied(Path::new("pdfium.dll")).to_string();
        assert!(text.contains("WriteDacl"), "{text}");
        assert!(text.contains("pdfium.dll"), "{text}");
    }
}
