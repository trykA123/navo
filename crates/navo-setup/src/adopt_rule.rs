//! Pure adopt rules for E8: the chain shape a helper run must leave, and the root's listing.

use crate::placement::{Chain, NameEq};

/// Byte offset of `FileName` in `FILE_ID_BOTH_DIR_INFO`; `pin_adopt.rs` asserts it on Windows.
pub(crate) const NAME_OFFSET: usize = 104;
/// Byte offset of `FileNameLength` (in bytes) in `FILE_ID_BOTH_DIR_INFO`.
const NAME_LENGTH_OFFSET: usize = 60;

/// Why a before/after chain pair cannot be adopted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShapeError {
    InvalidChain,
    Identity,
    Reparse,
}

/// Checks that `after` is `before` with every missing name now an existing folder.
///
/// Fails with `InvalidChain` when `before` has no parent or nothing to create, `Reparse` when any
/// link is a reparse point, and `Identity` when ids, the count or a name under `names` differ.
pub(crate) fn adopt_shape(
    before: &Chain,
    after: &Chain,
    names: &dyn NameEq,
) -> Result<(), ShapeError> {
    if before.existing.is_empty() || before.missing.is_empty() {
        return Err(ShapeError::InvalidChain);
    }
    if before
        .existing
        .iter()
        .chain(&after.existing)
        .any(|link| link.reparse)
    {
        return Err(ShapeError::Reparse);
    }
    let (kept, new) = after
        .existing
        .split_at(before.existing.len().min(after.existing.len()));
    let same_parents = kept.len() == before.existing.len()
        && kept
            .iter()
            .zip(&before.existing)
            .all(|(now, then)| now.id == then.id);
    let same_names = new.len() == before.missing.len()
        && new
            .iter()
            .zip(&before.missing)
            .all(|(link, name)| names.same(&link.name, name));
    // A folder the helper made is new, so its id cannot repeat an earlier link's.
    let fresh_ids = new.iter().enumerate().all(|(index, link)| {
        after.existing[..kept.len() + index]
            .iter()
            .all(|earlier| earlier.id != link.id)
    });
    if !after.missing.is_empty() || !same_parents || !same_names || !fresh_ids {
        return Err(ShapeError::Identity);
    }
    Ok(())
}

/// Whether one `FileIdBothDirectoryRestartInfo` batch lists only `.` and `..`.
///
/// `None` means the buffer is malformed (an entry or name runs past its end); callers fail closed.
pub(crate) fn listing_empty(buffer: &[u8]) -> Option<bool> {
    let mut start = 0_usize;
    loop {
        let entry = buffer.get(start..)?;
        let next = read_u32(entry, 0)?;
        let length = read_u32(entry, NAME_LENGTH_OFFSET)?;
        if length % 2 != 0 {
            return None;
        }
        let name = entry.get(NAME_OFFSET..NAME_OFFSET.checked_add(length)?)?;
        let units = name
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair));
        let dots = units.clone().all(|unit| unit == u16::from(b'.'));
        if !dots || !(1..=2).contains(&units.count()) {
            return Some(false);
        }
        if next == 0 {
            return Some(true);
        }
        if next < NAME_OFFSET + length {
            return None;
        }
        start = start.checked_add(next)?;
    }
}

/// Reads a little-endian `u32` at `offset` as a `usize`; `None` past the end.
fn read_u32(bytes: &[u8], offset: usize) -> Option<usize> {
    let raw = bytes.get(offset..offset.checked_add(4)?)?;
    usize::try_from(u32::from_le_bytes(raw.try_into().ok()?)).ok()
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::path::PathBuf;

    use super::{NAME_OFFSET, ShapeError, adopt_shape, listing_empty};
    use crate::placement::{Chain, FileId, Link, NameEq};

    /// ASCII case-insensitive names for the pure tests; Windows supplies the ordinal rule.
    struct AsciiNames;

    impl NameEq for AsciiNames {
        fn same(&self, a: &OsStr, b: &OsStr) -> bool {
            a.to_string_lossy()
                .eq_ignore_ascii_case(&b.to_string_lossy())
        }
    }

    fn link(name: &str, id: u128) -> Link {
        Link {
            path: PathBuf::from(name),
            name: OsString::from(name),
            id: FileId { volume: 7, id },
            reparse: false,
        }
    }

    fn before() -> Chain {
        Chain {
            existing: vec![link("C:", 1), link("Program Files", 2)],
            missing: vec![OsString::from("Navo"), OsString::from("app")],
        }
    }

    fn after() -> Chain {
        Chain {
            existing: vec![
                link("C:", 1),
                link("Program Files", 2),
                link("NAVO", 3),
                link("app", 4),
            ],
            missing: Vec::new(),
        }
    }

    fn shape(before: &Chain, after: &Chain) -> Result<(), ShapeError> {
        adopt_shape(before, after, &AsciiNames)
    }

    #[test]
    fn a_completed_chain_adopts_with_case_folded_names() {
        assert_eq!(shape(&before(), &after()), Ok(()));
    }

    #[test]
    fn a_before_chain_without_parent_or_missing_names_is_invalid() {
        let mut no_parent = before();
        no_parent.existing.clear();
        assert_eq!(shape(&no_parent, &after()), Err(ShapeError::InvalidChain));
        let mut nothing_missing = before();
        nothing_missing.missing.clear();
        assert_eq!(
            shape(&nothing_missing, &after()),
            Err(ShapeError::InvalidChain)
        );
    }

    #[test]
    fn a_reparse_link_anywhere_is_reparse() {
        for index in 0..4 {
            let mut changed = after();
            changed.existing[index].reparse = true;
            assert_eq!(
                shape(&before(), &changed),
                Err(ShapeError::Reparse),
                "link {index}"
            );
        }
        let mut parent = before();
        parent.existing[1].reparse = true;
        assert_eq!(shape(&parent, &after()), Err(ShapeError::Reparse));
    }

    #[test]
    fn a_swapped_parent_or_volume_is_identity() {
        let mut parent = after();
        parent.existing[1].id.id = 9;
        assert_eq!(shape(&before(), &parent), Err(ShapeError::Identity));
        let mut volume = after();
        volume.existing[0].id.volume = 8;
        assert_eq!(shape(&before(), &volume), Err(ShapeError::Identity));
    }

    #[test]
    fn a_renamed_short_long_or_unfinished_chain_is_identity() {
        let mut renamed = after();
        renamed.existing[3] = link("app2", 4);
        assert_eq!(shape(&before(), &renamed), Err(ShapeError::Identity));
        let mut short = after();
        short.existing.pop();
        assert_eq!(shape(&before(), &short), Err(ShapeError::Identity));
        let mut long = after();
        long.existing.push(link("extra", 5));
        assert_eq!(shape(&before(), &long), Err(ShapeError::Identity));
        let mut unfinished = after();
        unfinished.missing.push(OsString::from("app"));
        assert_eq!(shape(&before(), &unfinished), Err(ShapeError::Identity));
    }

    #[test]
    fn a_new_folder_reusing_an_earlier_id_is_identity() {
        let mut reused = after();
        reused.existing[3].id = reused.existing[1].id;
        assert_eq!(shape(&before(), &reused), Err(ShapeError::Identity));
    }

    /// Appends one aligned `FILE_ID_BOTH_DIR_INFO` entry; `last` leaves `NextEntryOffset` zero.
    fn push_entry(buffer: &mut Vec<u8>, name: &str, last: bool) {
        let start = buffer.len();
        let wide: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let size = (NAME_OFFSET + wide.len()).next_multiple_of(8);
        buffer.resize(start + size, 0);
        let next = if last {
            0
        } else {
            u32::try_from(size).unwrap()
        };
        buffer[start..start + 4].copy_from_slice(&next.to_le_bytes());
        let length = u32::try_from(wide.len()).unwrap().to_le_bytes();
        buffer[start + 60..start + 64].copy_from_slice(&length);
        buffer[start + NAME_OFFSET..start + NAME_OFFSET + wide.len()].copy_from_slice(&wide);
    }

    fn listing(names: &[&str]) -> Vec<u8> {
        let mut buffer = Vec::new();
        for (index, name) in names.iter().enumerate() {
            push_entry(&mut buffer, name, index + 1 == names.len());
        }
        buffer.resize(4096, 0xAA);
        buffer
    }

    #[test]
    fn only_dot_entries_is_empty() {
        assert_eq!(listing_empty(&listing(&[".", ".."])), Some(true));
        assert_eq!(listing_empty(&listing(&[".."])), Some(true));
    }

    #[test]
    fn any_other_entry_is_not_empty() {
        assert_eq!(
            listing_empty(&listing(&[".", "..", "navo.exe"])),
            Some(false)
        );
        assert_eq!(listing_empty(&listing(&["...", ".", ".."])), Some(false));
        assert_eq!(listing_empty(&listing(&[".", ".x"])), Some(false));
    }

    #[test]
    fn a_malformed_listing_fails_closed() {
        let mut overlong = listing(&[".", ".."]);
        overlong[60..64].copy_from_slice(&5000_u32.to_le_bytes());
        assert_eq!(listing_empty(&overlong), None);
        let mut odd = listing(&["."]);
        odd[60..64].copy_from_slice(&3_u32.to_le_bytes());
        assert_eq!(listing_empty(&odd), None);
        let mut past_end = listing(&[".", ".."]);
        past_end[0..4].copy_from_slice(&4090_u32.to_le_bytes());
        assert_eq!(listing_empty(&past_end), None);
        assert_eq!(listing_empty(&[0_u8; 50]), None);
    }
}
