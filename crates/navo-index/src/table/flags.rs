use std::ops::BitOr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flags(u16);

impl Flags {
    pub const NONE: Flags = Flags(0);
    pub const FOLDER: Flags = Flags(1);
    pub const HIDDEN: Flags = Flags(2);
    pub const SYSTEM: Flags = Flags(4);
    pub const REPARSE: Flags = Flags(8);
    pub(super) const DELETED: u16 = 16;

    pub const fn contains(self, other: Flags) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn bits(self) -> u16 {
        self.0
    }

    pub(super) const fn from_stored(bits: u16) -> Flags {
        Flags(bits & !Self::DELETED)
    }
}

impl BitOr for Flags {
    type Output = Flags;

    fn bitor(self, rhs: Flags) -> Flags {
        Flags(self.0 | rhs.0)
    }
}
