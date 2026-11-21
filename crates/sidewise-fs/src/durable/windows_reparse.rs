//! Windows `RealDisk` guards: refuses files a filter driver serves (cloud, dedup, HSM and
//! WOF placeholders) and device paths, so every write step sees plain bytes on a volume.

const REPARSE_POINT: u32 = 0x400;
const OFFLINE: u32 = 0x1000;
const RECALL_ON_OPEN: u32 = 0x4_0000;
const RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
/// Bit 29 of a reparse tag marks a name surrogate: a symlink, junction or mount point.
const NAME_SURROGATE: u32 = 0x2000_0000;

/// Why a file with these attributes and reparse tag is refused, or `None` for a plain file.
/// A filter serves a placeholder's bytes; opened without following, it can read as zeros.
pub(super) fn refusal(attributes: u32, tag: u32) -> Option<&'static str> {
    if attributes & REPARSE_POINT != 0 {
        return Some(if tag & NAME_SURROGATE != 0 {
            "path is a link"
        } else {
            "file is a placeholder (cloud, dedup, HSM or WOF-compressed); not supported"
        });
    }
    if attributes & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0 {
        return Some("file is offline or not downloaded; not supported");
    }
    None
}

/// True when verbatim UTF-16 `units` name a file on a volume: `\\?\X:\`, `\\?\UNC\` or
/// `\\?\Volume{`. Device paths (`\\.\`, `\\?\GLOBALROOT`, `\\?\C:`) are false.
pub(super) fn on_a_volume(units: &[u16]) -> bool {
    let Some(rest) = strip_ascii(units, r"\\?\", false) else {
        return false;
    };
    match rest {
        [letter, 0x3A, 0x5C, ..] => u8::try_from(*letter).is_ok_and(|b| b.is_ascii_alphabetic()),
        _ => {
            strip_ascii(rest, r"UNC\", true).is_some()
                || strip_ascii(rest, "Volume{", true).is_some()
        }
    }
}

/// `units` without the ASCII `prefix`, or `None` when they do not start with it.
fn strip_ascii<'a>(units: &'a [u16], prefix: &str, ignore_case: bool) -> Option<&'a [u16]> {
    let head = units.get(..prefix.len())?;
    let same = head.iter().zip(prefix.bytes()).all(|(&unit, byte)| {
        u8::try_from(unit)
            .is_ok_and(|unit| unit == byte || (ignore_case && unit.eq_ignore_ascii_case(&byte)))
    });
    same.then(|| &units[prefix.len()..])
}

/// Why a file with these attributes is refused when its volume cannot report a reparse tag,
/// or `None` for a plain file. Without the tag a reparse point cannot be told apart: refuse it.
pub(super) fn untagged_refusal(attributes: u32) -> Option<&'static str> {
    if attributes & REPARSE_POINT != 0 {
        return Some("file is a reparse point whose tag the volume does not report; not supported");
    }
    refusal(attributes, 0)
}

/// Refuses a link, placeholder or offline file behind an open handle with `InvalidInput`.
/// exFAT and FAT reject the tag query (error 87); there the attributes alone decide. Any
/// other failed query is returned, so the check fails closed.
#[cfg(windows)]
pub(super) fn check_plain(file: &std::fs::File) -> std::io::Result<()> {
    let why = match tag_info(file) {
        Ok((attributes, tag)) => refusal(attributes, tag),
        Err(error) if crate::native::identity::unsupported(&error) => {
            untagged_refusal(basic_attributes(file)?)
        }
        Err(error) => return Err(error),
    };
    match why {
        Some(why) => Err(super::real::refused(why)),
        None => Ok(()),
    }
}

/// The attributes and reparse tag of an open handle (`FileAttributeTagInfo`).
/// Fails with the OS error, 87 on a volume without reparse support.
#[cfg(windows)]
#[allow(unsafe_code, reason = "FileAttributeTagInfo is a handle query")]
fn tag_info(file: &std::fs::File) -> std::io::Result<(u32, u32)> {
    use std::mem::{MaybeUninit, size_of};
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_TAG_INFO, FileAttributeTagInfo, GetFileInformationByHandleEx,
    };

    let mut info = MaybeUninit::<FILE_ATTRIBUTE_TAG_INFO>::zeroed();
    // SAFETY: the handle and the correctly sized output buffer live through the query.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileAttributeTagInfo,
            info.as_mut_ptr().cast(),
            size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: a successful query initializes FILE_ATTRIBUTE_TAG_INFO.
    let info = unsafe { info.assume_init() };
    Ok((info.FileAttributes, info.ReparseTag))
}

/// The attributes of an open handle (`FileBasicInfo`), which every local file system answers.
/// Fails with the OS error.
#[cfg(windows)]
#[allow(unsafe_code, reason = "FileBasicInfo is a handle query")]
fn basic_attributes(file: &std::fs::File) -> std::io::Result<u32> {
    use std::mem::{MaybeUninit, size_of};
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_BASIC_INFO, FileBasicInfo, GetFileInformationByHandleEx,
    };

    let mut info = MaybeUninit::<FILE_BASIC_INFO>::zeroed();
    // SAFETY: the handle and the correctly sized output buffer live through the query.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileBasicInfo,
            info.as_mut_ptr().cast(),
            size_of::<FILE_BASIC_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: a successful query initializes FILE_BASIC_INFO.
    Ok(unsafe { info.assume_init() }.FileAttributes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    #[test]
    fn plain_files_pass_and_links_placeholders_and_offline_files_are_refused() {
        assert_eq!(refusal(0x20, 0), None);
        assert_eq!(refusal(0x80, 0), None);
        let symlink = refusal(0x400, 0xA000_000C).unwrap();
        let junction = refusal(0x400, 0xA000_0003).unwrap();
        assert!(symlink.contains("link") && junction.contains("link"));
        for tag in [0x8000_0017, 0x8000_0013, 0x9000_601A, 0xC000_0004] {
            let why = refusal(0x420, tag).unwrap_or_default();
            assert!(why.contains("placeholder"), "tag {tag:#x}: {why:?}");
        }
        for attribute in [0x1000, 0x4_0000, 0x40_0000] {
            let why = refusal(0x20 | attribute, 0).unwrap_or_default();
            assert!(why.contains("offline"), "attribute {attribute:#x}: {why:?}");
        }
    }

    #[test]
    fn without_a_tag_every_reparse_point_and_offline_file_is_refused() {
        assert_eq!(untagged_refusal(0x20), None);
        assert_eq!(untagged_refusal(0x80), None);
        for attributes in [0x400, 0x420, 0x410] {
            let why = untagged_refusal(attributes).unwrap_or_default();
            assert!(why.contains("reparse point"), "{attributes:#x}: {why:?}");
        }
        let why = untagged_refusal(0x20 | 0x40_0000).unwrap_or_default();
        assert!(why.contains("offline"), "{why:?}");
    }

    #[test]
    fn only_drive_unc_and_volume_paths_are_on_a_volume() {
        for good in [
            r"\\?\C:\x",
            r"\\?\z:\",
            r"\\?\UNC\srv\share\x",
            r"\\?\unc\srv\share",
            r"\\?\Volume{0a1b}\x",
        ] {
            assert!(on_a_volume(&wide(good)), "{good}");
        }
        for device in [
            r"\\.\C:\x",
            r"//./C:/x",
            r"\\?\GLOBALROOT\Device\HarddiskVolume1\x",
            r"\\?\globalroot\Device\Null",
            r"\\?\Global\C:\x",
            r"\\?\C:",
            r"\\?\pipe\x",
            r"C:\x",
            r"\\?\1:\x",
        ] {
            assert!(!on_a_volume(&wide(device)), "{device}");
        }
    }
}
