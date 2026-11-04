//! The Recycle Bin veto: the shell may delete only the verified item, and only into the bin.

use super::act::Identity;

/// `TSF_DELETE_RECYCLE_IF_POSSIBLE`; the shell clears it when a delete would be permanent.
pub(super) const RECYCLE_FLAG: u32 = 0x80;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Veto {
    Allow,
    Permanent,
    OtherItem,
}

/// `found` is the identity at the shell's path now; `None` when it could not be opened.
pub(super) fn pre_delete(flags: u32, found: Option<Identity>, expected: Identity) -> Veto {
    if flags & RECYCLE_FLAG == 0 {
        Veto::Permanent
    } else if found != Some(expected) {
        Veto::OtherItem
    } else {
        Veto::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEM: Identity = Identity {
        volume: 7,
        file: 42,
    };

    #[test]
    fn small_only_a_recycle_of_the_verified_item_is_allowed() {
        assert_eq!(pre_delete(RECYCLE_FLAG, Some(ITEM), ITEM), Veto::Allow);
        assert_eq!(
            pre_delete(RECYCLE_FLAG | 0x1, Some(ITEM), ITEM),
            Veto::Allow
        );
    }

    #[test]
    fn small_a_delete_without_the_recycle_flag_is_vetoed() {
        for flags in [0, 0x1, 0x40, !RECYCLE_FLAG] {
            assert_eq!(pre_delete(flags, Some(ITEM), ITEM), Veto::Permanent);
        }
    }

    #[test]
    fn small_another_or_unopenable_item_is_vetoed() {
        let other_file = Identity {
            volume: 7,
            file: 43,
        };
        let other_volume = Identity {
            volume: 8,
            file: 42,
        };
        for found in [Some(other_file), Some(other_volume), None] {
            assert_eq!(pre_delete(RECYCLE_FLAG, found, ITEM), Veto::OtherItem);
        }
    }
}
