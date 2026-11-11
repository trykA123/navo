//! Pure DACL rule: when one ACE lets another user change a folder or a drive root.

/// `INHERIT_ONLY_ACE`: the ACE applies only to children, not to this object.
const INHERIT_ONLY_ACE: u32 = 0x08;
/// `DELETE`: the right to delete or rename the folder itself.
const DELETE: u32 = 0x0001_0000;
/// `FILE_DELETE_CHILD`: the right to delete the folder's children.
const FILE_DELETE_CHILD: u32 = 0x0000_0040;
/// `WRITE_DAC`: the right to change the folder's permissions.
const WRITE_DAC: u32 = 0x0004_0000;
/// `WRITE_OWNER`: the right to change the folder's owner.
const WRITE_OWNER: u32 = 0x0008_0000;
/// `GENERIC_ALL`: every right, mapped when the ACE is applied.
const GENERIC_ALL: u32 = 0x1000_0000;
/// `GENERIC_WRITE`: the generic write rights, mapped when the ACE is applied.
const GENERIC_WRITE: u32 = 0x4000_0000;

/// Rights that let another user change a normal folder.
const FOLDER_RIGHTS: u32 = DELETE | WRITE_DAC | WRITE_OWNER | GENERIC_ALL | GENERIC_WRITE;
/// Rights that open a drive root; a root cannot be renamed, so DELETE and GENERIC_WRITE are out.
const ROOT_RIGHTS: u32 = FILE_DELETE_CHILD | WRITE_DAC | WRITE_OWNER | GENERIC_ALL;

/// ACE types that deny access.
const DENY_ACE_TYPES: [u8; 4] = [1, 6, 10, 12];
/// ACE types that only audit or alarm.
const AUDIT_ACE_TYPES: [u8; 8] = [2, 3, 7, 8, 13, 14, 15, 16];

/// Whether one ACE gives an untrusted SID rights over a folder, or over a drive root.
#[expect(
    clippy::too_many_arguments,
    reason = "the ACE type, flags, mask, root and trust are the rule's whole table"
)]
pub(crate) fn ace_opens(ace_type: u8, flags: u32, mask: u32, is_root: bool, trusted: bool) -> bool {
    if trusted || DENY_ACE_TYPES.contains(&ace_type) || AUDIT_ACE_TYPES.contains(&ace_type) {
        return false;
    }
    // Object, callback and compound ACEs hide their SID, so any such ACE is taken as open.
    if ace_type != 0 {
        return true;
    }
    let applies = flags & INHERIT_ONLY_ACE == 0;
    let rights = if is_root {
        ROOT_RIGHTS
    } else {
        FOLDER_RIGHTS | FILE_DELETE_CHILD
    };
    applies && mask & rights != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every rule point: trust, ACE type, inherit-only, folder and root rights.
    #[test]
    fn the_ace_rule_covers_types_flags_masks_and_roots() {
        assert!(ace_opens(0, 0, DELETE, false, false));
        assert!(!ace_opens(0, 0, DELETE, false, true));
        assert!(!ace_opens(0, INHERIT_ONLY_ACE, DELETE, false, false));
        assert!(ace_opens(0, 0, FILE_DELETE_CHILD, false, false));
        assert!(!ace_opens(
            0,
            INHERIT_ONLY_ACE,
            FILE_DELETE_CHILD,
            false,
            false
        ));

        assert!(ace_opens(0, 0, FILE_DELETE_CHILD, true, false));
        assert!(ace_opens(0, 0, WRITE_DAC, true, false));
        assert!(ace_opens(0, 0, WRITE_OWNER, true, false));
        assert!(ace_opens(0, 0, GENERIC_ALL, true, false));
        assert!(!ace_opens(0, 0, DELETE, true, false));
        assert!(!ace_opens(0, 0, GENERIC_WRITE, true, false));
        assert!(!ace_opens(0, INHERIT_ONLY_ACE, GENERIC_ALL, true, false));

        for ace_type in [4, 5, 9, 11] {
            assert!(ace_opens(ace_type, 0, 0, false, false), "{ace_type}");
            assert!(!ace_opens(ace_type, 0, 0, false, true), "{ace_type}");
        }
        assert!(ace_opens(99, 0, 0, false, false));
        for ace_type in [1, 6, 10, 12, 2, 3, 7, 8, 13, 14, 15, 16] {
            assert!(
                !ace_opens(ace_type, 0, u32::MAX, false, false),
                "{ace_type}"
            );
        }
    }
}
