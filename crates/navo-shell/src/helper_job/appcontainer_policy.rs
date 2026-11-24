//! The platform-neutral half of the PDF helper's AppContainer grant (S2c1): which files are granted,
//! when access already exists, how a failed step is named, and the fail-closed rule.
//! `appcontainer_win.rs` calls Win32.

use std::fmt;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// The fixed AppContainer name; Windows derives the same SID from it on every call.
pub const CONTAINER_NAME: &str = "Navo.PdfHelper";

/// `FILE_GENERIC_READ | FILE_GENERIC_EXECUTE` (0x120089 | 0x1200A0), the only rights granted.
pub const READ_EXECUTE: u32 = 0x0012_00A9;

/// ALL APPLICATION PACKAGES: an allow ACE for it reaches every AppContainer, ours included.
pub const ALL_APP_PACKAGES: &str = "S-1-15-2-1";

/// Everyone: the container's token holds it, so a deny ACE for it blocks the container too.
pub const EVERYONE: &str = "S-1-1-0";

/// The PDF engine the helper loads; it sits next to navo.exe.
const PDFIUM_DLL: &str = "pdfium.dll";

/// `ERROR_INVALID_ACL`: a NULL DACL, or ACL bytes whose sizes or SIDs do not add up.
pub const ERROR_INVALID_ACL: i32 = 1336;

/// `ACCESS_ALLOWED_ACE_TYPE` and the `INHERIT_ONLY_ACE` flag (winnt.h).
const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
const INHERIT_ONLY_ACE: u8 = 0x08;

/// ACE types whose SID directly follows the header and mask: allowed, denied and their callback
/// forms (winnt.h).
const PLAIN_SID_ACE_TYPES: [u8; 4] = [0, 1, 9, 0x0A];

/// Object ACE types (allowed, denied and their callback forms): `Flags` and up to two GUIDs sit
/// between the mask and the SID.
const OBJECT_ACE_TYPES: [u8; 4] = [5, 6, 0x0B, 0x0C];

/// Allow ACE types: they never block the grant. Only a plain allow counts towards it.
const ALLOW_ACE_TYPES: [u8; 4] = [0, 5, 9, 0x0B];

/// Deny ACE types: plain, object and their callback forms. They block when their trustee reaches
/// the container; any type in none of these lists blocks (fail closed).
const DENY_ACE_TYPES: [u8; 4] = [1, 6, 0x0A, 0x0C];

/// Byte sizes in an ACL: its header, an ACE header plus mask, an object ACE's `Flags`, a GUID,
/// and a SID's fixed part (revision, count, 6-byte authority).
const ACL_HEADER: usize = 8;
/// ACL revisions Windows accepts (`ACL_REVISION` 2 up to `ACL_REVISION_DS` 4).
const ACL_REVISIONS: [u8; 3] = [2, 3, 4];
/// A self-relative descriptor's header: revision, padding, control, then owner, group, SACL and DACL offsets.
const SD_HEADER: usize = 20;
const SD_REVISION: u8 = 1;
const SD_DACL_OFFSET: usize = 16;
const SE_DACL_PRESENT: u16 = 0x0004;
const SE_DACL_AUTO_INHERITED: u16 = 0x0400;
const SE_DACL_PROTECTED: u16 = 0x1000;
const SE_SELF_RELATIVE: u16 = 0x8000;
const ACE_HEADER_AND_MASK: usize = 8;
const OBJECT_FLAGS: usize = 4;
const GUID: usize = 16;
const SID_HEADER: usize = 8;

/// `ACE_OBJECT_TYPE_PRESENT`, `ACE_INHERITED_OBJECT_TYPE_PRESENT`, `SID_REVISION` and
/// `SID_MAX_SUB_AUTHORITIES` (winnt.h).
const OBJECT_TYPE_PRESENT: u32 = 0x1;
const INHERITED_OBJECT_TYPE_PRESENT: u32 = 0x2;
const SID_REVISION: u8 = 1;
const SID_MAX_SUB_AUTHORITIES: usize = 15;

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
    Everyone,
    /// Any other SID; a deny ACE for it does not block the grant.
    Other,
    /// An ACE of a type whose layout Navo does not decode; it blocks the grant.
    Unknown,
}

/// One DACL entry: its type, flags, access mask and trustee.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AceView {
    pub ace_type: u8,
    pub flags: u8,
    pub mask: u32,
    pub trustee: Trustee,
}

/// True when the DACL already lets the container read and execute the file: plain allow ACEs for
/// the container or ALL APPLICATION PACKAGES (inherited too) cover every bit, no deny ACE that
/// reaches the container (its SID, ALL APPLICATION PACKAGES or Everyone) touches them, and no ACE
/// is of an unknown type. Callback and object allows are conditional: they neither grant nor block.
pub fn already_granted(aces: &[AceView]) -> bool {
    let mut allowed = 0;
    for ace in aces.iter().filter(|ace| ace.flags & INHERIT_ONLY_ACE == 0) {
        let reaches = ace.trustee != Trustee::Other;
        if ace.ace_type == ACCESS_ALLOWED_ACE_TYPE {
            if matches!(ace.trustee, Trustee::Container | Trustee::AllAppPackages) {
                allowed |= ace.mask;
            }
        } else if DENY_ACE_TYPES.contains(&ace.ace_type) {
            if reaches && ace.mask & DENY_RELEVANT != 0 {
                return false;
            }
        } else if !ALLOW_ACE_TYPES.contains(&ace.ace_type) {
            return false;
        }
    }
    allowed & READ_EXECUTE == READ_EXECUTE
}

/// Where a self-relative security descriptor's DACL lies, and the DACL control bits a write keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaclSpan {
    /// The DACL's header and `AclSize` bytes, as offsets into the descriptor.
    pub range: Range<usize>,
    /// The descriptor's `SE_DACL_PROTECTED` (no inheritance from the parent folder) and
    /// `SE_DACL_AUTO_INHERITED` bits; every other control bit is cleared.
    pub control: u16,
}

/// Finds the DACL in `descriptor`, the self-relative bytes Windows wrote into a buffer Navo owns.
/// Only `descriptor.len()` bounds the result: the DACL offset must be DWORD-aligned and past the
/// header, and the ACL header and its `AclSize` must fit. Fails with `ERROR_INVALID_ACL` otherwise,
/// and for a missing or NULL DACL, because merging into it would drop "everyone" access.
pub fn locate_dacl(descriptor: &[u8]) -> Result<DaclSpan, i32> {
    let control = read_u16(descriptor, 2)?;
    let offset =
        usize::try_from(read_u32(descriptor, SD_DACL_OFFSET)?).map_err(|_| ERROR_INVALID_ACL)?;
    let header_ok = read_u8(descriptor, 0)? == SD_REVISION && control & SE_SELF_RELATIVE != 0;
    let present = control & SE_DACL_PRESENT != 0 && offset != 0;
    if !header_ok || !present || offset < SD_HEADER || !offset.is_multiple_of(4) {
        return Err(ERROR_INVALID_ACL);
    }
    let acl = descriptor.get(offset..).ok_or(ERROR_INVALID_ACL)?;
    let revision = read_u8(acl, 0)?;
    let size = usize::from(read_u16(acl, 2)?);
    let size_ok = size >= ACL_HEADER && size.is_multiple_of(4) && size <= acl.len();
    if !ACL_REVISIONS.contains(&revision) || !size_ok {
        return Err(ERROR_INVALID_ACL);
    }
    Ok(DaclSpan {
        range: offset..offset + size,
        control: control & (SE_DACL_PROTECTED | SE_DACL_AUTO_INHERITED),
    })
}

/// Describes every ACE in the raw `acl` bytes (header included). The whole layout is checked
/// first; only then does `names(known, sid)` say whether an ACE's SID bytes name `known`, and it
/// only sees SIDs that are valid and lie inside their ACE. Fails with `ERROR_INVALID_ACL` when the
/// header, an `AceSize` or a SID breaks the layout.
pub fn parse_dacl(
    acl: &[u8],
    mut names: impl FnMut(Trustee, &[u8]) -> bool,
) -> Result<Vec<AceView>, i32> {
    let size = usize::from(read_u16(acl, 2)?);
    let count = read_u16(acl, 4)?;
    let acl = acl
        .get(..size)
        .filter(|_| size >= ACL_HEADER)
        .ok_or(ERROR_INVALID_ACL)?;
    let mut checked = Vec::with_capacity(count.into());
    let mut offset = ACL_HEADER;
    for _ in 0..count {
        let ace_size = usize::from(read_u16(acl, offset + 2)?);
        let ace = acl
            .get(offset..offset + ace_size)
            .filter(|_| ace_size >= ACE_HEADER_AND_MASK && ace_size % 4 == 0)
            .ok_or(ERROR_INVALID_ACL)?;
        checked.push(checked_ace(ace)?);
        offset += ace_size;
    }
    let known = [
        Trustee::Container,
        Trustee::AllAppPackages,
        Trustee::Everyone,
    ];
    let mut name = |sid: Option<&[u8]>| match sid {
        Some(sid) => known
            .into_iter()
            .find(|&known| names(known, sid))
            .unwrap_or(Trustee::Other),
        None => Trustee::Unknown,
    };
    Ok(checked
        .into_iter()
        .map(|(mut view, sid)| {
            view.trustee = name(sid);
            view
        })
        .collect())
}

/// Checks one ACE whose bytes are exactly `ace` and returns its view (trustee not named yet) and
/// its SID. The SID offset follows the type; an object ACE's `Flags` say which GUIDs come first.
/// An unknown type has no SID read.
fn checked_ace(ace: &[u8]) -> Result<(AceView, Option<&[u8]>), i32> {
    let ace_type = read_u8(ace, 0)?;
    let sid_start = if PLAIN_SID_ACE_TYPES.contains(&ace_type) {
        Some(ACE_HEADER_AND_MASK)
    } else if OBJECT_ACE_TYPES.contains(&ace_type) {
        let object_flags = read_u32(ace, ACE_HEADER_AND_MASK)?;
        let guids = [OBJECT_TYPE_PRESENT, INHERITED_OBJECT_TYPE_PRESENT]
            .into_iter()
            .filter(|bit| object_flags & bit != 0)
            .count();
        Some(ACE_HEADER_AND_MASK + OBJECT_FLAGS + GUID * guids)
    } else {
        None
    };
    let sid = sid_start.map(|start| sid_at(ace, start)).transpose()?;
    let view = AceView {
        ace_type,
        flags: read_u8(ace, 1)?,
        mask: read_u32(ace, 4)?,
        trustee: Trustee::Unknown,
    };
    Ok((view, sid))
}

/// The SID that starts at `start` in `ace`, exactly as long as its sub-authority count says.
/// Fails unless its revision is 1, it has at most 15 sub-authorities and it ends inside `ace`.
fn sid_at(ace: &[u8], start: usize) -> Result<&[u8], i32> {
    let revision = read_u8(ace, start)?;
    let subs = usize::from(read_u8(ace, start + 1)?);
    if revision != SID_REVISION || subs > SID_MAX_SUB_AUTHORITIES {
        return Err(ERROR_INVALID_ACL);
    }
    ace.get(start..start + SID_HEADER + 4 * subs)
        .ok_or(ERROR_INVALID_ACL)
}

/// The byte at `at`, or `ERROR_INVALID_ACL` past the end.
fn read_u8(bytes: &[u8], at: usize) -> Result<u8, i32> {
    bytes.get(at).copied().ok_or(ERROR_INVALID_ACL)
}

/// The little-endian `u16` at `at`, or `ERROR_INVALID_ACL` past the end.
fn read_u16(bytes: &[u8], at: usize) -> Result<u16, i32> {
    let field = bytes
        .get(at..at + 2)
        .and_then(|field| field.try_into().ok());
    field.map(u16::from_le_bytes).ok_or(ERROR_INVALID_ACL)
}

/// The little-endian `u32` at `at`, or `ERROR_INVALID_ACL` past the end.
fn read_u32(bytes: &[u8], at: usize) -> Result<u32, i32> {
    let field = bytes
        .get(at..at + 4)
        .and_then(|field| field.try_into().ok());
    field.map(u32::from_le_bytes).ok_or(ERROR_INVALID_ACL)
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
        assert_eq!(EVERYONE, "S-1-1-0");
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
        assert!(!already_granted(&[
            ace(1, 0, 0x20, Trustee::Container),
            allow
        ]));
        assert!(!already_granted(&[
            allow,
            ace(1, 0, 0x8000_0000, Trustee::Everyone)
        ]));
        assert!(already_granted(&[
            ace(1, 0, 0x0004_0000, Trustee::Container),
            allow
        ]));
        assert!(already_granted(&[
            ace(1, 0x08, READ_EXECUTE, Trustee::Everyone),
            allow
        ]));
    }

    #[test]
    fn a_deny_for_the_container_all_app_packages_everyone_or_an_unread_sid_blocks() {
        let allow = ace(0, 0, READ_EXECUTE, Trustee::Container);
        for trustee in [
            Trustee::Container,
            Trustee::AllAppPackages,
            Trustee::Everyone,
            Trustee::Unknown,
        ] {
            let deny = ace(1, 0, READ_EXECUTE, trustee);
            assert!(!already_granted(&[allow, deny]), "{trustee:?}");
        }
        // An object deny (type 6) for the container blocks like a plain one.
        assert!(!already_granted(&[
            ace(6, 0, 0x20, Trustee::Container),
            allow
        ]));
    }

    #[test]
    fn a_deny_for_an_unrelated_trustee_keeps_the_grant() {
        let allow = ace(0, 0, READ_EXECUTE, Trustee::Container);
        assert!(already_granted(&[
            ace(1, 0, READ_EXECUTE, Trustee::Other),
            allow
        ]));
        assert!(already_granted(&[
            allow,
            ace(0x0A, 0x10, 0x8000_0000, Trustee::Other)
        ]));
    }

    #[test]
    fn an_allow_for_everyone_does_not_reach_the_container() {
        assert!(!already_granted(&[ace(
            0,
            0,
            READ_EXECUTE,
            Trustee::Everyone
        )]));
        assert!(!already_granted(&[ace(
            0,
            0,
            READ_EXECUTE,
            Trustee::Unknown
        )]));
    }

    /// A SID with identifier `authority` and `subs` sub-authorities, as Windows lays it out.
    fn sid(authority: u8, subs: &[u32]) -> Vec<u8> {
        let mut bytes = vec![1, subs.len() as u8, 0, 0, 0, 0, 0, authority];
        subs.iter().for_each(|sub| bytes.extend(sub.to_le_bytes()));
        bytes
    }

    fn known_sid(known: Trustee) -> Vec<u8> {
        match known {
            Trustee::Container => sid(15, &[2, 7, 8, 9]),
            Trustee::AllAppPackages => sid(15, &[2, 1]),
            Trustee::Everyone => sid(1, &[0]),
            Trustee::Other | Trustee::Unknown => sid(5, &[11]),
        }
    }

    /// One raw ACE: header, mask and `body`, padded to a DWORD boundary.
    fn raw_ace(ace_type: u8, mask: u32, body: &[u8]) -> Vec<u8> {
        let mut bytes = vec![ace_type, 0, 0, 0];
        bytes.extend(mask.to_le_bytes());
        bytes.extend(body);
        bytes.resize(bytes.len().next_multiple_of(4), 0);
        let size = bytes.len() as u16;
        bytes[2..4].copy_from_slice(&size.to_le_bytes());
        bytes
    }

    /// An object ACE body: `flags`, one GUID per flag bit set, then the SID.
    fn object_body(flags: u32, sid: &[u8]) -> Vec<u8> {
        let guids = (flags & 1) + (flags >> 1 & 1);
        let mut body = flags.to_le_bytes().to_vec();
        body.extend(vec![0xAB; 16 * guids as usize]);
        body.extend(sid);
        body
    }

    /// A raw ACL: revision 2, its size, the ACE count, then `aces`.
    fn raw_acl(aces: &[Vec<u8>]) -> Vec<u8> {
        let size = 8 + aces.iter().map(Vec::len).sum::<usize>();
        let mut bytes = vec![2, 0];
        bytes.extend((size as u16).to_le_bytes());
        bytes.extend((aces.len() as u16).to_le_bytes());
        bytes.extend([0, 0]);
        aces.iter().for_each(|ace| bytes.extend(ace));
        bytes
    }

    fn parse(acl: &[u8]) -> Result<Vec<AceView>, i32> {
        parse_dacl(acl, |known, found| found == known_sid(known).as_slice())
    }

    fn trustees(acl: &[u8]) -> Vec<Trustee> {
        parse(acl).unwrap().iter().map(|ace| ace.trustee).collect()
    }

    #[test]
    fn plain_and_callback_aces_name_their_trustee_and_keep_type_flags_and_mask() {
        let known = [
            Trustee::Container,
            Trustee::AllAppPackages,
            Trustee::Everyone,
            Trustee::Other,
        ];
        for ace_type in [0, 1, 9, 0x0A] {
            let aces: Vec<_> = known
                .iter()
                .map(|&trustee| raw_ace(ace_type, 0x20, &known_sid(trustee)))
                .collect();
            assert_eq!(trustees(&raw_acl(&aces)), known, "{ace_type}");
        }
        let mut inherited = raw_ace(1, 0x8000_0000, &known_sid(Trustee::Everyone));
        inherited[1] = 0x18;
        let parsed = parse(&raw_acl(&[inherited])).unwrap();
        assert_eq!(parsed, [ace(1, 0x18, 0x8000_0000, Trustee::Everyone)]);
    }

    #[test]
    fn a_relevant_callback_deny_blocks_and_a_callback_allow_does_not() {
        let allow = raw_ace(0, READ_EXECUTE, &known_sid(Trustee::Container));
        let mut callback = known_sid(Trustee::Everyone);
        callback.extend(b"app data");
        let deny = raw_ace(0x0A, 0x20, &callback);
        assert_eq!(
            trustees(&raw_acl(&[allow.clone(), deny.clone()])),
            [Trustee::Container, Trustee::Everyone]
        );
        assert!(!already_granted(
            &parse(&raw_acl(&[allow.clone(), deny])).unwrap()
        ));
        let callback_allow = raw_ace(9, READ_EXECUTE, &known_sid(Trustee::Container));
        let object_allow = raw_ace(
            5,
            READ_EXECUTE,
            &object_body(1, &known_sid(Trustee::Container)),
        );
        let acl = raw_acl(&[callback_allow, object_allow, allow]);
        assert!(already_granted(&parse(&acl).unwrap()));
    }

    #[test]
    fn an_object_deny_is_matched_by_the_sid_after_its_guids() {
        let allow = raw_ace(0, READ_EXECUTE, &known_sid(Trustee::Container));
        for (ace_type, flags) in [(6, 0), (6, 1), (6, 2), (6, 3), (0x0C, 3)] {
            let own = raw_ace(
                ace_type,
                0x20,
                &object_body(flags, &known_sid(Trustee::Container)),
            );
            let acl = raw_acl(&[allow.clone(), own]);
            assert_eq!(trustees(&acl), [Trustee::Container, Trustee::Container]);
            assert!(
                !already_granted(&parse(&acl).unwrap()),
                "{ace_type} {flags}"
            );
            let other = raw_ace(
                ace_type,
                0x20,
                &object_body(flags, &known_sid(Trustee::Other)),
            );
            let acl = raw_acl(&[allow.clone(), other]);
            assert_eq!(trustees(&acl), [Trustee::Container, Trustee::Other]);
            assert!(already_granted(&parse(&acl).unwrap()), "{ace_type} {flags}");
        }
    }

    #[test]
    fn an_unknown_ace_type_blocks_whatever_its_mask() {
        let allow = raw_ace(0, READ_EXECUTE, &known_sid(Trustee::Container));
        let label = raw_ace(0x11, 0, &known_sid(Trustee::Other));
        let acl = raw_acl(&[allow, label]);
        assert_eq!(trustees(&acl), [Trustee::Container, Trustee::Unknown]);
        assert!(!already_granted(&parse(&acl).unwrap()));
    }

    #[test]
    fn a_malformed_acl_is_an_error_and_no_sid_is_compared() {
        let good = raw_ace(1, 0x20, &known_sid(Trustee::Container));
        let mut cases: Vec<(&str, Vec<u8>)> = Vec::new();
        cases.push(("no header", vec![2, 0, 8]));
        let mut short = raw_acl(std::slice::from_ref(&good));
        short.truncate(short.len() - 1);
        cases.push(("AclSize past the buffer", short));
        let mut tiny = raw_acl(&[]);
        tiny[2] = 4;
        cases.push(("AclSize below the header", tiny));
        let mut count = raw_acl(std::slice::from_ref(&good));
        count[4] = 2;
        cases.push(("AceCount past AclSize", count));
        let mut truncated = raw_acl(std::slice::from_ref(&good));
        truncated[2] -= 4;
        cases.push(("ACE past AclSize", truncated));
        for (name, ace_size) in [
            ("AceSize unaligned", 18u16),
            ("AceSize below header", 4),
            ("AceSize zero", 0),
        ] {
            let mut acl = raw_acl(std::slice::from_ref(&good));
            acl[10..12].copy_from_slice(&ace_size.to_le_bytes());
            cases.push((name, acl));
        }
        let mut overrun = good.clone();
        overrun[9] = 5;
        cases.push(("SID past the ACE", raw_acl(&[overrun])));
        let mut revision = good.clone();
        revision[8] = 2;
        cases.push(("SID revision", raw_acl(&[revision])));
        let mut subs = good.clone();
        subs[9] = 16;
        cases.push(("too many sub-authorities", raw_acl(&[subs])));
        cases.push(("no SID", raw_acl(&[raw_ace(1, 0x20, &[])])));
        let guids = raw_ace(6, 0x20, &3u32.to_le_bytes());
        cases.push(("object GUIDs past the ACE", raw_acl(&[guids])));
        for (name, acl) in cases {
            let result = parse_dacl(&acl, |_, _| panic!("{name}: SID compared"));
            assert_eq!(result, Err(ERROR_INVALID_ACL), "{name}");
        }
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

    /// A self-relative descriptor with `control` (plus `SE_SELF_RELATIVE`), its DACL at offset 20.
    fn raw_descriptor(control: u16, acl: &[u8]) -> Vec<u8> {
        let mut bytes = vec![1, 0];
        bytes.extend((control | 0x8000).to_le_bytes());
        bytes.extend([0; 12]);
        bytes.extend(20u32.to_le_bytes());
        bytes.extend(acl);
        bytes
    }

    fn container_acl() -> Vec<u8> {
        raw_acl(&[raw_ace(0, READ_EXECUTE, &known_sid(Trustee::Container))])
    }

    #[test]
    fn the_dacl_is_located_by_its_offset_and_size_with_its_kept_control_bits() {
        let acl = container_acl();
        let mut descriptor = raw_descriptor(0x0004, &acl);
        descriptor.extend([0xEE; 8]);
        let span = DaclSpan {
            range: 20..20 + acl.len(),
            control: 0,
        };
        assert_eq!(locate_dacl(&descriptor), Ok(span));
        let protected = raw_descriptor(0x1404, &acl);
        assert_eq!(locate_dacl(&protected).map(|span| span.control), Ok(0x1400));
        let other_bits = raw_descriptor(0x0A04, &acl);
        assert_eq!(locate_dacl(&other_bits).map(|span| span.control), Ok(0));
    }

    #[test]
    fn a_dacl_outside_the_buffer_unaligned_or_null_is_refused() {
        let acl = container_acl();
        let good = raw_descriptor(0x0004, &acl);
        let with = |at: usize, value: &[u8]| {
            let mut bytes = good.clone();
            bytes[at..at + value.len()].copy_from_slice(value);
            bytes
        };
        let past = (good.len() as u32 + 4).to_le_bytes();
        let size = |size: usize| (size as u16).to_le_bytes();
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("descriptor header truncated", good[..19].to_vec()),
            ("descriptor revision", with(0, &[2])),
            ("absolute descriptor", with(2, &0x0004u16.to_le_bytes())),
            ("no DACL", with(2, &0x8000u16.to_le_bytes())),
            ("NULL DACL", with(16, &0u32.to_le_bytes())),
            ("offset inside the header", with(16, &8u32.to_le_bytes())),
            ("offset unaligned", with(16, &22u32.to_le_bytes())),
            ("offset past the buffer", with(16, &past)),
            (
                "offset far past the buffer",
                with(16, &0xFFFF_FFFCu32.to_le_bytes()),
            ),
            ("ACL header truncated", good[..22].to_vec()),
            ("ACL revision", with(20, &[1])),
            ("AclSize past the buffer", with(22, &size(acl.len() + 4))),
            ("AclSize unaligned", with(22, &size(acl.len() - 2))),
            ("AclSize below the header", with(22, &size(4))),
        ];
        for (name, descriptor) in cases {
            assert_eq!(locate_dacl(&descriptor), Err(ERROR_INVALID_ACL), "{name}");
        }
    }
}
