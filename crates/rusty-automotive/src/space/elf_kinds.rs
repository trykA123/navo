use std::collections::HashMap;

use super::{Kind, unnumbered};
use crate::elf::Elf;
use crate::map::OutputSection;

pub(super) struct ElfKinds<'a>(HashMap<(u64, &'a str), Kind>);

impl<'a> ElfKinds<'a> {
    pub(super) fn new(elf: Option<&'a Elf>) -> Self {
        let sections = elf.map_or(&[][..], |elf| elf.sections.as_slice());
        Self(
            sections
                .iter()
                .filter(|section| section.is_loaded() && section.size > 0)
                .map(|section| {
                    (
                        (section.address, section.name.as_str()),
                        Kind::of_section(section),
                    )
                })
                .collect(),
        )
    }

    pub(super) fn of(&self, section: &OutputSection) -> Kind {
        let name = unnumbered(&section.name);
        self.0
            .get(&(section.address, name))
            .copied()
            .unwrap_or_else(|| Kind::of_name(name))
    }
}
