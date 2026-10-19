use std::collections::{HashMap, HashSet};

use rusty_automotive::map::{MapModel, MemoryRegion, OutputSection, Symbol};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) enum Scope {
    #[default]
    All,
    Region(String),
    Object(String),
}

impl Scope {
    pub(super) fn label(&self) -> Option<String> {
        match self {
            Self::All => None,
            Self::Region(name) => Some(format!("region {name}")),
            Self::Object(name) => Some(format!("object {name}")),
        }
    }
}

type Span<'a> = (Option<&'a str>, u64, u64);

#[derive(Default)]
struct Spans<'a>(Vec<Span<'a>>);

impl<'a> Spans<'a> {
    fn push(&mut self, space: Option<&'a str>, start: u64, size: u64) {
        self.0
            .push((space, start, start.saturating_add(size.max(1))));
    }

    fn merged(mut self) -> Self {
        self.0.sort_unstable();
        let mut merged: Vec<Span<'a>> = Vec::new();
        for (space, start, end) in self.0 {
            match merged.last_mut() {
                Some((last, _, last_end)) if *last == space && start <= *last_end => {
                    *last_end = (*last_end).max(end);
                }
                _ => merged.push((space, start, end)),
            }
        }
        Self(merged)
    }

    fn holds(&self, space: Option<&str>, address: u64) -> bool {
        let at = self
            .0
            .partition_point(|(s, start, _)| (*s, *start) <= (space, address));
        at.checked_sub(1)
            .and_then(|before| self.0.get(before))
            .is_some_and(|(s, _, end)| *s == space && address < *end)
    }
}

pub(super) struct Filter<'a> {
    scope: &'a Scope,
    regions: HashSet<&'a str>,
    sections: HashSet<&'a str>,
    spans: Spans<'a>,
}

fn space_of<'a>(
    by_name: &HashMap<&str, Vec<&'a OutputSection>>,
    (name, address): (&str, u64),
) -> Option<&'a str> {
    let sections = by_name.get(name)?;
    let holding = sections
        .iter()
        .find(|s| address >= s.address && address - s.address < s.size.max(1));
    holding
        .or_else(|| sections.first())
        .and_then(|section| section.address_space.as_deref())
}

impl<'a> Filter<'a> {
    pub(super) fn new(model: &'a MapModel, scope: &'a Scope) -> Self {
        let mut filter = Self {
            scope,
            regions: HashSet::new(),
            sections: HashSet::new(),
            spans: Spans::default(),
        };
        match scope {
            Scope::All => {}
            Scope::Region(name) => filter.in_region(model, name),
            Scope::Object(name) => filter.in_object(model, name),
        }
        filter.spans = std::mem::take(&mut filter.spans).merged();
        filter
    }

    fn in_region(&mut self, model: &'a MapModel, name: &'a str) {
        self.regions.insert(name);
        for section in model
            .sections
            .iter()
            .filter(|section| section.region.as_deref() == Some(name))
        {
            self.spans.push(
                section.address_space.as_deref(),
                section.address,
                section.size,
            );
        }
    }

    fn in_object(&mut self, model: &'a MapModel, name: &str) {
        let mut by_name: HashMap<&str, Vec<&OutputSection>> = HashMap::new();
        for section in &model.sections {
            by_name.entry(&section.name).or_default().push(section);
        }
        for part in model
            .contributions
            .iter()
            .filter(|part| part.object_file == name)
        {
            self.sections.insert(&part.output_section);
            self.regions.extend(part.region.as_deref());
            if let Some(address) = part.address {
                let space = space_of(&by_name, (&part.output_section, address));
                self.spans.push(space, address, part.size);
            }
        }
    }

    pub(super) fn region(&self, region: &MemoryRegion) -> bool {
        matches!(self.scope, Scope::All) || self.regions.contains(region.name.as_str())
    }

    pub(super) fn section(&self, section: &OutputSection) -> bool {
        match self.scope {
            Scope::All => true,
            Scope::Region(_) => section
                .region
                .as_deref()
                .is_some_and(|region| self.regions.contains(region)),
            Scope::Object(_) => self.sections.contains(section.name.as_str()),
        }
    }

    pub(super) fn symbol(&self, symbol: &Symbol) -> bool {
        matches!(self.scope, Scope::All)
            || self
                .spans
                .holds(symbol.address_space.as_deref(), symbol.address)
    }
}
