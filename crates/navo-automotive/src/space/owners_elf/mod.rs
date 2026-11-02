mod claims;
mod files;
mod from_map;
mod map_owners;
mod prepare;
mod section;

use std::ops::Range;

use self::files::Interner;
pub(super) use self::files::{plain_section, unnumbered};
pub use self::map_owners::owners_of_map;
use self::prepare::Context;
use super::Kind;
use super::kind::takes_no_address_space;
use crate::{Elf, Lines, MapModel};

pub const NOT_ATTRIBUTED: &str = "(not attributed)";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameRef {
    Symbol(u32),
    Section(u32),
    Contribution(u32),
    Unattributed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owner {
    pub start: u64,
    pub size: u64,
    pub name: NameRef,
    pub kind: Kind,
    pub section: u32,
    pub object: Option<u32>,
    pub source_file: Option<u32>,
    pub estimated: bool,
    pub image: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Provenance {
    pub has_symbols: bool,
    pub has_lines: bool,
    pub from_map: u64,
    pub from_sections: u64,
    pub from_symbols: u64,
    pub estimated: u64,
    pub not_attributed: u64,
    pub load_images: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Owners {
    pub items: Vec<Owner>,
    pub provenance: Provenance,
    files: Vec<String>,
}

#[derive(Default)]
struct Builder {
    items: Vec<Owner>,
    files: Interner,
    provenance: Provenance,
}

#[must_use]
pub fn owners(elf: &Elf, lines: Option<&Lines>, map: Option<&MapModel>) -> Owners {
    let context = Context::new(elf, lines, map);
    let mut builder = Builder::default();
    builder.provenance.has_symbols = !context.stripped;
    builder.provenance.has_lines = lines.is_some_and(|lines| !lines.files.is_empty());
    for (at, section) in elf.sections.iter().enumerate() {
        let is_placed = section.is_loaded() && section.size > 0 && !takes_no_address_space(section);
        if is_placed && Kind::of_section(section) != Kind::Fill {
            section::build(&context, &mut builder, at);
        }
    }
    builder.items.sort_by_key(|owner| owner.start);
    Owners {
        items: builder.items,
        provenance: builder.provenance,
        files: builder.files.items,
    }
}

impl Owners {
    #[must_use]
    pub fn in_span(&self, span: Range<u64>) -> &[Owner] {
        let from = self.items.partition_point(|owner| owner.start < span.start);
        let to = self.items.partition_point(|owner| owner.start < span.end);
        &self.items[from..to.max(from)]
    }

    #[must_use]
    pub fn object(&self, owner: &Owner) -> Option<&str> {
        self.file(owner.object)
    }

    #[must_use]
    pub fn source_file(&self, owner: &Owner) -> Option<&str> {
        self.file(owner.source_file)
    }

    #[must_use]
    pub fn name<'a>(&self, owner: &Owner, elf: &'a Elf, map: Option<&'a MapModel>) -> &'a str {
        self.name_in(owner, Some(elf), map)
    }

    #[must_use]
    pub fn name_in<'a>(
        &self,
        owner: &Owner,
        elf: Option<&'a Elf>,
        map: Option<&'a MapModel>,
    ) -> &'a str {
        match owner.name {
            NameRef::Symbol(at) => elf
                .and_then(|elf| elf.symbols.get(at as usize).map(|s| elf.name(s)))
                .unwrap_or(""),
            NameRef::Section(at) => elf
                .and_then(|elf| elf.sections.get(at as usize))
                .map_or("", |section| section.name.as_str()),
            NameRef::Contribution(at) => map
                .and_then(|map| map.contributions.get(at as usize))
                .map_or("", |part| unnumbered(&part.section)),
            NameRef::Unattributed => NOT_ATTRIBUTED,
        }
    }

    fn file(&self, at: Option<u32>) -> Option<&str> {
        self.files.get(at? as usize).map(String::as_str)
    }
}
