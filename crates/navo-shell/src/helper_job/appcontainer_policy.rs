//! The platform-neutral half of the PDF helper's AppContainer grant (S2c1): which files are granted,
//! when access already exists, how a failed step is named, and the fail-closed rule.
//! `appcontainer_win.rs` calls Win32.

use std::fmt;
use std::path::{Path, PathBuf};

/// The fixed AppContainer name; Windows derives the same SID from it on every call.
pub const CONTAINER_NAME: &str = "Navo.PdfHelper";

/// `FILE_GENERIC_READ | FILE_GENERIC_EXECUTE` (0x120089 | 0x1200A0), the only rights granted.
pub const READ_EXECUTE: u32 = 0x0012_00A9;

/// ALL APPLICATION PACKAGES: an allow ACE for it reaches every AppContainer, ours included.
pub const ALL_APP_PACKAGES: &str = "S-1-15-2-1";

/// The PDF engine the helper loads; it sits next to navo.exe.
const PDFIUM_DLL: &str = "pdfium.dll";

/// `ACCESS_ALLOWED_ACE_TYPE` and the `INHERIT_ONLY_ACE` flag (winnt.h).
const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
const INHERIT_ONLY_ACE: u8 = 0x08;

/// A deny ACE blocks the grant when it touches read+execute or any generic or MAXIMUM_ALLOWED bit.
const DENY_RELEVANT: u32 = READ_EXECUTE | 0xF200_0000;

/// `FILE_ATTRIBUTE_DIRECTORY` and `FILE_ATTRIBUTE_REPARSE_POINT` (winnt.h).
const ATTRIBUTE_DIRECTORY: u32 = 0x10;
const ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

/// `ERROR_STOPPED_ON_SYMLINK` and `ERROR_DIRECTORY_NOT_SUPPORTED`: why a target is refused.
const ERROR_STOPPED_ON_SYMLINK: i32 = 681;
const ERROR_DIRECTORY_NOT_SUPPORTED: i32 = 336;

/// Win32 errors a retry may clear: sharing and lock violations, low memory or system resources.
const SHARING_VIOLATION: u32 = 32;
const LOCK_VIOLATION: u32 = 33;
const NOT_ENOUGH_MEMORY: u32 = 8;
const OUTOFMEMORY: u32 = 14;
const NO_SYSTEM_RESOURCES: u32 = 1450;

/// The step of a grant that failed; the diagnostics log names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantStep {
    /// The helper exe's path has no file name or no folder, so the targets cannot be named.
    LocateExe,
    /// Registering the AppContainer profile failed; `CreateProcessW` needs it (S2c2).
    CreateProfile,
    DeriveSid,
    FormatSid,
    OpenFile,
    /// The target is a reparse point (a symlink could redirect the grant) or a folder.
    CheckTarget,
    ReadDacl,
    ScanDacl,
    /// Reopening the checked handle with WRITE_DAC failed (the user may not own the file).
    OpenForWrite,
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

/// Whose SID an ACE names, as far as the grant cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trustee {
    Container,
    AllAppPackages,
    Other,
}

/// One DACL entry: its type, flags, access mask and trustee.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AceView {
    pub ace_type: u8,
    pub flags: u8,
    pub mask: u32,
    pub trustee: Trustee,
}

/// True when the DACL already lets the container read and execute the file: allow ACEs for the
/// container or ALL APPLICATION PACKAGES (inherited too) cover every bit and no deny ACE touches them.
pub fn already_granted(aces: &[AceView]) -> bool {
    let mut allowed = 0;
    for ace in aces.iter().filter(|ace| ace.flags & INHERIT_ONLY_ACE == 0) {
        if ace.ace_type != ACCESS_ALLOWED_ACE_TYPE {
            // Any deny-like entry that touches the rights is treated as blocking: fail closed.
            if ace.mask & DENY_RELEVANT != 0 {
                return false;
            }
        } else if ace.trustee != Trustee::Other {
            allowed |= ace.mask;
        }
    }
    allowed & READ_EXECUTE == READ_EXECUTE
}

/// Refuses a target whose `attributes` mark a reparse point or a folder; returns the Win32 error.
pub fn check_plain_file(attributes: u32) -> Result<(), i32> {
    if attributes & ATTRIBUTE_REPARSE_POINT != 0 {
        Err(ERROR_STOPPED_ON_SYMLINK)
    } else if attributes & ATTRIBUTE_DIRECTORY != 0 {
        Err(ERROR_DIRECTORY_NOT_SUPPORTED)
    } else {
        Ok(())
    }
}

/// The only files ever granted: `exe` and `pdfium.dll` in its folder. Never the PDF, never a
/// folder. Fails at `LocateExe` when `exe` has no file name or no folder.
pub fn helper_targets(exe: &Path) -> Result<[PathBuf; 2], GrantError> {
    let folder = exe.parent().filter(|dir| !dir.as_os_str().is_empty());
    match (folder, exe.file_name()) {
        (Some(folder), Some(_)) => Ok([exe.to_path_buf(), folder.join(PDFIUM_DLL)]),
        _ => Err(GrantError::new(GrantStep::LocateExe, Some(exe), 0)),
    }
}

/// Grants the helper `exe` being started and the `pdfium.dll` beside it with `grant`, in that
/// order, and fails closed: the first error stops the run and is returned, so the helper must not start.
pub fn grant_helper_files<F>(exe: &Path, mut grant: F) -> Result<(), GrantError>
where
    F: FnMut(&Path) -> Result<GrantOutcome, GrantError>,
{
    for target in helper_targets(exe)? {
        grant(&target)?;
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

    fn ace(ace_type: u8, flags: u8, mask: u32, trustee: Trustee) -> AceView {
        AceView {
            ace_type,
            flags,
            mask,
            trustee,
        }
    }

    #[test]
    fn the_container_name_is_pinned_because_the_sid_derives_from_it() {
        assert_eq!(CONTAINER_NAME, "Navo.PdfHelper");
        assert_eq!(ALL_APP_PACKAGES, "S-1-15-2-1");
    }

    #[test]
    fn an_allow_for_the_container_or_all_app_packages_is_already_granted() {
        let own = ace(0, 0, READ_EXECUTE, Trustee::Container);
        assert!(already_granted(&[own]));
        // ALL APPLICATION PACKAGES, inherited, as under Program Files.
        let any = ace(0, 0x10, READ_EXECUTE | 0x2, Trustee::AllAppPackages);
        assert!(already_granted(&[any]));
        let read = ace(0, 0, 0x0012_0089, Trustee::Container);
        let exec = ace(0, 0x10, 0x0012_00A0, Trustee::AllAppPackages);
        assert!(already_granted(&[read, exec]));
    }

    #[test]
    fn a_partial_foreign_or_inherit_only_allow_is_not_a_grant() {
        assert!(!already_granted(&[]));
        assert!(!already_granted(&[ace(
            0,
            0,
            0x0012_0089,
            Trustee::Container
        )]));
        assert!(!already_granted(&[ace(0, 0, READ_EXECUTE, Trustee::Other)]));
        assert!(!already_granted(&[ace(
            0,
            0x08,
            READ_EXECUTE,
            Trustee::Container
        )]));
    }

    #[test]
    fn a_deny_touching_read_execute_means_not_granted() {
        let allow = ace(0, 0, READ_EXECUTE, Trustee::Container);
        assert!(!already_granted(&[ace(1, 0, 0x20, Trustee::Other), allow]));
        assert!(!already_granted(&[
            allow,
            ace(1, 0, 0x8000_0000, Trustee::Other)
        ]));
        assert!(already_granted(&[
            ace(1, 0, 0x0004_0000, Trustee::Other),
            allow
        ]));
        assert!(already_granted(&[
            ace(1, 0x08, READ_EXECUTE, Trustee::Other),
            allow
        ]));
    }

    #[test]
    fn the_targets_are_exactly_the_exe_and_pdfium_next_to_it() {
        let exe = Path::new("/opt/navo/navo.exe");
        let targets = helper_targets(exe).unwrap();
        assert_eq!(
            targets,
            [
                PathBuf::from("/opt/navo/navo.exe"),
                PathBuf::from("/opt/navo/pdfium.dll")
            ]
        );
    }

    #[test]
    fn an_exe_path_without_a_folder_or_name_is_refused() {
        for exe in ["navo.exe", "/", ""] {
            let error = helper_targets(Path::new(exe)).unwrap_err();
            assert_eq!(error.step, GrantStep::LocateExe, "{exe}");
        }
    }

    #[test]
    fn the_helper_exe_and_its_pdfium_are_granted_in_order() {
        let exe = Path::new("/opt/navo/navo.exe");
        let mut seen = Vec::new();
        let result = grant_helper_files(exe, |path| {
            seen.push(path.to_path_buf());
            Ok(GrantOutcome::Added)
        });
        assert_eq!(result, Ok(()));
        assert_eq!(seen, [exe.to_path_buf(), exe.with_file_name("pdfium.dll")]);
    }

    #[test]
    fn a_failed_grant_stops_the_run_and_is_returned() {
        let exe = Path::new("/opt/navo/navo.exe");
        let mut calls = 0;
        let result = grant_helper_files(exe, |path| {
            calls += 1;
            Err(denied(path))
        });
        assert_eq!(result, Err(denied(exe)));
        assert_eq!(calls, 1);
    }

    #[test]
    fn a_failure_on_pdfium_still_fails_closed() {
        let exe = Path::new("/opt/navo/navo.exe");
        let dll = Path::new("/opt/navo/pdfium.dll");
        let result = grant_helper_files(exe, |path| {
            if path == dll {
                Err(denied(path))
            } else {
                Ok(GrantOutcome::AlreadyGranted)
            }
        });
        assert_eq!(result, Err(denied(dll)));
    }

    #[test]
    fn a_reparse_point_or_folder_is_refused_and_a_plain_file_passes() {
        assert_eq!(check_plain_file(0x20), Ok(()));
        assert_eq!(check_plain_file(0x80), Ok(()));
        assert_eq!(check_plain_file(0x400 | 0x20), Err(681));
        assert_eq!(check_plain_file(0x10), Err(336));
    }

    #[test]
    fn every_step_is_named_in_the_message() {
        let steps = [
            GrantStep::LocateExe,
            GrantStep::CreateProfile,
            GrantStep::DeriveSid,
            GrantStep::FormatSid,
            GrantStep::OpenFile,
            GrantStep::CheckTarget,
            GrantStep::ReadDacl,
            GrantStep::ScanDacl,
            GrantStep::OpenForWrite,
            GrantStep::BuildDacl,
            GrantStep::WriteDacl,
        ];
        for step in steps {
            let text = GrantError::new(step, None, 5).to_string();
            assert!(text.contains(&format!("{step:?}")), "{text}");
        }
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
