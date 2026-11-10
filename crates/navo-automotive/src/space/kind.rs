//! Classifies automotive memory allocations from ELF sections and names.

use object::elf::{SHF_TLS, SHF_WRITE, SHT_NOBITS};

use crate::elf::Section;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Code,
    Rodata,
    Data,
    Bss,
    Other,
    Fill,
}

impl Kind {
    pub const COUNT: usize = 6;

    #[must_use]
    pub fn index(self) -> usize {
        self as usize
    }

    #[must_use]
    /// Classifies an ELF section by its allocation, execution, and storage flags.
    pub fn of_section(section: &Section) -> Self {
        if is_fill_name(&section.name) {
            Self::Fill
        } else if !section.is_loaded() {
            Self::Other
        } else if section.is_executable() {
            Self::Code
        } else if section.kind == SHT_NOBITS.0 && !is_copied_data(section) {
            Self::Bss
        } else if section.flags & SHF_WRITE.0 != 0 {
            Self::Data
        } else {
            Self::Rodata
        }
    }

    #[must_use]
    pub fn of_name(name: &str) -> Self {
        if name == "*fill*" || is_fill_name(name) {
            return Self::Fill;
        }
        if let Some(inner) = name
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            return match Self::of_name(inner) {
                Self::Data => Self::Rodata,
                other => other,
            };
        }
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

/// Checks whether a section represents uninitialized thread-local storage.
pub(super) fn takes_no_address_space(section: &Section) -> bool {
    section.kind == SHT_NOBITS.0 && section.flags & SHF_TLS.0 != 0
}

fn is_fill_name(name: &str) -> bool {
    name.starts_with("$fill")
}

/// Checks whether writable section names identify initialized copied data.
fn is_copied_data(section: &Section) -> bool {
    let name = section.name.as_str();
    section.flags & SHF_WRITE.0 != 0
        && ["data", "zdata", "sdata"].iter().any(|stem| {
            name.strip_prefix('.')
                .and_then(|rest| rest.strip_prefix(stem))
                .is_some_and(|tail| tail.is_empty() || tail.starts_with('.'))
        })
}

#[cfg(test)]
mod tests {
    use object::elf::{SHF_ALLOC, SHT_PROGBITS};

    use super::*;

    fn section(name: &str, kind: u32) -> Section {
        Section {
            name: name.to_owned(),
            kind,
            flags: SHF_ALLOC.0 | SHF_WRITE.0,
            address: 0,
            size: 4,
            offset: 0,
        }
    }

    #[test]
    fn nobits_zdata_is_data() {
        assert_eq!(
            Kind::of_section(&section(".zdata.foo", SHT_NOBITS.0)),
            Kind::Data
        );
        assert_eq!(
            Kind::of_section(&section(".sdata", SHT_NOBITS.0)),
            Kind::Data
        );
    }

    #[test]
    fn nobits_bss_stays_bss() {
        assert_eq!(
            Kind::of_section(&section(".bss.foo", SHT_NOBITS.0)),
            Kind::Bss
        );
        assert_eq!(
            Kind::of_section(&section(".database", SHT_NOBITS.0)),
            Kind::Bss
        );
    }

    #[test]
    fn bracketed_rom_copies_are_rodata_by_name() {
        assert_eq!(Kind::of_name("[.data.Com.State]"), Kind::Rodata);
        assert_eq!(Kind::of_name("[.zdata.Com]"), Kind::Rodata);
        assert_eq!(Kind::of_name(".data.Com.State"), Kind::Data);
    }

    #[test]
    fn progbits_data_stays_data() {
        assert_eq!(
            Kind::of_section(&section(".data", SHT_PROGBITS.0)),
            Kind::Data
        );
    }
}
