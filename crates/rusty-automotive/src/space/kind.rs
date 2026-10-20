use object::elf::{SHF_TLS, SHF_WRITE, SHT_NOBITS};

use crate::elf::Section;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Code,
    Rodata,
    Data,
    Bss,
    Other,
}

impl Kind {
    pub const COUNT: usize = 5;

    #[must_use]
    pub fn index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub fn of_section(section: &Section) -> Self {
        if !section.is_loaded() {
            Self::Other
        } else if section.is_executable() {
            Self::Code
        } else if section.kind == SHT_NOBITS {
            Self::Bss
        } else if section.flags & u64::from(SHF_WRITE) != 0 {
            Self::Data
        } else {
            Self::Rodata
        }
    }

    #[must_use]
    pub fn of_name(name: &str) -> Self {
        let lead = name
            .trim_start_matches('.')
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let small = lead.strip_prefix(['s', 'z', 'l']).unwrap_or(&lead);
        [lead.as_str(), small]
            .into_iter()
            .find_map(Self::of_stem)
            .unwrap_or(Self::Other)
    }

    fn of_stem(stem: &str) -> Option<Self> {
        match stem {
            "text" | "code" => Some(Self::Code),
            "rodata" => Some(Self::Rodata),
            "data" => Some(Self::Data),
            "bss" => Some(Self::Bss),
            _ => None,
        }
    }
}

pub(super) fn takes_no_address_space(section: &Section) -> bool {
    section.kind == SHT_NOBITS && section.flags & u64::from(SHF_TLS) != 0
}
