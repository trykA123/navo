use std::ops::BitOr;

pub(crate) type StoredFlags = u8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flags(u16);

impl Flags {
    pub const NONE: Flags = Flags(0);
    pub const FOLDER: Flags = Flags(1);
    pub const HIDDEN: Flags = Flags(2);
    pub const SYSTEM: Flags = Flags(4);
    pub const REPARSE: Flags = Flags(8);
    pub(super) const DELETED: StoredFlags = 16;
    pub(super) const VALID: StoredFlags = Self::FOLDER.stored()
        | Self::HIDDEN.stored()
        | Self::SYSTEM.stored()
        | Self::REPARSE.stored()
        | Self::DELETED;

    pub const fn contains(self, other: Flags) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn bits(self) -> u16 {
        self.0
    }

    pub(super) const fn stored(self) -> StoredFlags {
        self.0 as StoredFlags
    }

    pub(super) const fn from_stored(bits: StoredFlags) -> Flags {
        Flags(bits as u16 & !(Self::DELETED as u16))
    }
}

const _: () = assert!(
    (Flags::FOLDER.bits()
        | Flags::HIDDEN.bits()
        | Flags::SYSTEM.bits()
        | Flags::REPARSE.bits()
        | Flags::DELETED as u16)
        <= u8::MAX as u16,
    "every stored flag bit fits in one byte"
);

impl BitOr for Flags {
    type Output = Flags;

    fn bitor(self, rhs: Flags) -> Flags {
        Flags(self.0 | rhs.0)
    }
}

#[cfg(test)]
mod tests {
    use super::Flags;
    use crate::table::{NO_PARENT, NameTable};

    fn flags_column_bytes(table: &NameTable) -> usize {
        table
            .column_bytes()
            .into_iter()
            .find(|(column, _)| *column == "flags")
            .map(|(_, bytes)| bytes)
            .expect("the flags column is reported")
    }

    #[test]
    fn a_million_slot_table_uses_one_byte_of_flags_per_slot() {
        let slots = 1_000_000;
        let mut table = NameTable::with_capacity(slots, 0);
        let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
        for _ in 1..slots {
            table.push(root, "f", Flags::NONE).unwrap();
        }
        assert_eq!(table.slots() as usize, slots);
        assert_eq!(flags_column_bytes(&table), slots);
    }
}
