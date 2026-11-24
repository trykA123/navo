//! IW25 elevated helper, pure contract: its command line, argument checks, descriptor rule and exit
//! codes. Built on every OS; the Windows probe (E4–E8) launches, runs and verifies with it.

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

use crate::placement::FileId;
use crate::syntax::parse_candidate;

#[cfg(test)]
#[path = "elevate_tests.rs"]
mod tests;

/// The first argument that turns Setup into the elevated folder helper.
pub const HELPER_FLAG: &str = "--elevated-create";

/// SID of the built-in Administrators group, the only owner a helper-made folder may have.
const ADMINISTRATORS: &str = "S-1-5-32-544";
/// SID of the local SYSTEM account.
const SYSTEM: &str = "S-1-5-18";
/// `FILE_ALL_ACCESS` (`FA` in SDDL).
const FULL: u32 = 0x1F_01FF;
/// Modify without `WRITE_DAC` and `WRITE_OWNER` (`0x1301bf` in SDDL).
const MODIFY: u32 = 0x13_01BF;
/// `ACCESS_ALLOWED_ACE_TYPE`.
const ALLOW: u8 = 0;
/// `OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE` (`OICI`), nothing inherited or inherit-only.
const OICI: u8 = 0x3;

/// What the wizard asks the helper to create: the root, for which user, under which held parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperArgs {
    pub root: PathBuf,
    pub user_sid: String,
    pub parent: FileId,
}

/// One ACE as read back from a folder's DACL, with its SID in string form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AceView {
    pub kind: u8,
    pub flags: u8,
    pub mask: u32,
    pub sid: String,
}

/// The helper's process exit code, read by the wizard after the elevated run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HelperExit {
    Created = 0,
    NotElevated = 20,
    BadArguments = 21,
    Refused = 22,
    Changed = 23,
    Exists = 24,
    CreateFailed = 25,
}

impl HelperExit {
    /// Every variant, for the code lookup.
    const ALL: [Self; 7] = [
        Self::Created,
        Self::NotElevated,
        Self::BadArguments,
        Self::Refused,
        Self::Changed,
        Self::Exists,
        Self::CreateFailed,
    ];

    /// The process exit code for this result.
    pub fn code(self) -> u32 {
        u32::from(self as u8)
    }

    /// Maps a process exit code back to a result; `None` for any code the helper never returns.
    pub fn from_code(code: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|exit| exit.code() == code)
    }
}

/// Builds the helper's arguments as one Windows command line, starting with `HELPER_FLAG`.
/// Trailing `\` are stripped from the root so they cannot escape its closing quote; IW6 refuses `"`.
/// `None` when the root has no UTF-8 form or the helper's `parse_candidate` would refuse it.
pub fn helper_command_line(args: &HelperArgs) -> Option<String> {
    let root = args.root.to_str()?.trim_end_matches('\\');
    parse_candidate(root.as_ref()).ok()?;
    Some(format!(
        "{HELPER_FLAG} --root \"{root}\" --for {} --parent {:016x}:{:032x}",
        args.user_sid, args.parent.volume, args.parent.id
    ))
}

/// The helper's parameters for `ShellExecuteExW`: `helper_command_line` as NUL-terminated UTF-16.
/// Fails closed with `InvalidInput`, before any prompt, when the root is refused, the SID is not a
/// user SID (it is written unquoted) or a NUL would cut the line short.
pub fn launch_parameters(args: &HelperArgs) -> io::Result<Vec<u16>> {
    let refused = |what| io::Error::new(io::ErrorKind::InvalidInput, what);
    if !valid_user_sid(&args.user_sid) {
        return Err(refused("the helper's user SID is not a user SID"));
    }
    let line = helper_command_line(args).ok_or_else(|| refused("the helper refuses this root"))?;
    if line.contains('\0') {
        return Err(refused("the helper's command line holds a NUL"));
    }
    Ok(line.encode_utf16().chain(Some(0)).collect())
}

/// Parses the six arguments after `HELPER_FLAG`: `--root <path> --for <sid> --parent <vol>:<id>`.
/// Fails with `BadArguments` on any other count, order, root, SID or id shape.
pub fn parse_helper_args(args: &[OsString]) -> Result<HelperArgs, HelperExit> {
    let [root_key, root, for_key, sid, parent_key, parent] = args else {
        return Err(HelperExit::BadArguments);
    };
    if root_key != "--root" || for_key != "--for" || parent_key != "--parent" {
        return Err(HelperExit::BadArguments);
    }
    parse_candidate(root).map_err(|_| HelperExit::BadArguments)?;
    let user_sid = sid.to_str().filter(|sid| valid_user_sid(sid));
    let user_sid = user_sid.ok_or(HelperExit::BadArguments)?.to_owned();
    let parent = parent.to_str().and_then(parse_file_id);
    Ok(HelperArgs {
        root: PathBuf::from(root),
        user_sid,
        parent: parent.ok_or(HelperExit::BadArguments)?,
    })
}

/// Parses `<16 lowercase hex>:<32 lowercase hex>` into a folder id; `None` for any other shape.
fn parse_file_id(text: &str) -> Option<FileId> {
    let (volume, id) = text.split_once(':')?;
    let hex = |part: &str, width: usize| {
        part.len() == width && part.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    };
    if !hex(volume, 16) || !hex(id, 32) {
        return None;
    }
    Some(FileId {
        volume: u64::from_str_radix(volume, 16).ok()?,
        id: u128::from_str_radix(id, 16).ok()?,
    })
}

/// True for a local or domain user (`S-1-5-21-a-b-c-r`, `r >= 1000` or the built-in
/// Administrator `r == 500`) or an Entra ID user (`S-1-12-1-a-b-c-d`); each part a decimal `u32`
/// without leading zeros.
pub fn valid_user_sid(sid: &str) -> bool {
    if let Some(rest) = sid.strip_prefix("S-1-5-21-") {
        return u32_parts(rest).is_some_and(|parts| parts[3] == 500 || parts[3] >= 1000);
    }
    sid.strip_prefix("S-1-12-1-")
        .is_some_and(|rest| u32_parts(rest).is_some())
}

/// Splits `a-b-c-d` into four canonical decimal `u32` values; `None` for any other shape.
fn u32_parts(text: &str) -> Option<[u32; 4]> {
    let mut parts = [0; 4];
    let mut items = text.split('-');
    for slot in &mut parts {
        let item = items.next()?;
        let digits = !item.is_empty() && item.bytes().all(|b| b.is_ascii_digit());
        if !digits || (item.len() > 1 && item.starts_with('0')) {
            return None;
        }
        *slot = item.parse().ok()?;
    }
    items.next().is_none().then_some(parts)
}

/// The IW25 descriptor for a helper-made folder: owner Administrators, protected, SYSTEM and
/// Administrators full, the user Modify without `WRITE_DAC` or `WRITE_OWNER`, all inherited.
pub fn admin_root_sddl(sid: &str) -> String {
    format!("O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1301bf;;;{sid})")
}

/// True when a folder's security is exactly `admin_root_sddl(user)`: owner Administrators only
/// (a user owner holds `WRITE_DAC` implicitly), protected, and the three IW25 ACEs in any order.
pub fn admin_descriptor_ok(owner: &str, protected: bool, aces: &[AceView], user: &str) -> bool {
    if owner != ADMINISTRATORS || !protected || !valid_user_sid(user) || aces.len() != 3 {
        return false;
    }
    let expected = [(SYSTEM, FULL), (ADMINISTRATORS, FULL), (user, MODIFY)];
    expected.iter().all(|&(sid, mask)| {
        let matching = aces.iter().filter(|ace| ace.sid == sid).collect::<Vec<_>>();
        matches!(matching.as_slice(), [ace] if ace.kind == ALLOW && ace.flags == OICI && ace.mask == mask)
    })
}
