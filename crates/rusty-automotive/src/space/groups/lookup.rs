use super::module::Modules;
use super::{Grouping, OTHER, Origin};
use crate::space::{NOT_ATTRIBUTED, NameRef, Owner, Owners, unnumbered};

pub(super) struct Lookup<'a> {
    pub(super) owners: &'a Owners,
    pub(super) origin: Origin<'a>,
    pub(super) modules: &'a Modules,
    pub(super) grouping: Grouping,
}

impl<'a> Lookup<'a> {
    pub(super) fn key(&self, owner: &Owner) -> (&'a str, bool) {
        let plain = |name: Option<&'a str>| (name.unwrap_or(OTHER), false);
        match self.grouping {
            Grouping::SourceFile => plain(self.owners.source_file(owner)),
            Grouping::ObjectFile => plain(self.owners.object(owner)),
            Grouping::Section => plain(self.section(owner)),
            Grouping::Module => self.module(owner),
        }
    }

    pub(super) fn child(&self, owner: &Owner) -> &'a str {
        match (self.origin, self.grouping) {
            (Origin::Map(_), Grouping::Module) => self.owners.object(owner).unwrap_or(OTHER),
            _ => self.name(owner),
        }
    }

    fn module(&self, owner: &Owner) -> (&'a str, bool) {
        let name = self.name(owner);
        if name == NOT_ATTRIBUTED {
            return (NOT_ATTRIBUTED, false);
        }
        let section = match owner.name {
            NameRef::Symbol(_) => None,
            _ => self.section(owner),
        };
        let found = self
            .modules
            .of([self.owners.object(owner), section, Some(name)]);
        found.map_or((OTHER, false), |found| (found.name, found.listed))
    }

    fn name(&self, owner: &Owner) -> &'a str {
        match self.origin {
            Origin::Elf { elf, map } => self.owners.name_in(owner, Some(elf), map),
            Origin::Map(map) => self.owners.name_in(owner, None, Some(map)),
        }
    }

    fn section(&self, owner: &Owner) -> Option<&'a str> {
        let at = owner.section as usize;
        match self.origin {
            Origin::Elf { elf, .. } => elf.sections.get(at).map(|section| section.name.as_str()),
            Origin::Map(map) => map
                .sections
                .get(at)
                .map(|section| unnumbered(&section.name)),
        }
    }
}
