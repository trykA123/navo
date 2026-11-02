use std::collections::BTreeMap;

use super::{MapModel, Symbol};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectTotal<'a> {
    pub object_file: &'a str,
    pub region: Option<&'a str>,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionTotal<'a> {
    pub region: &'a str,
    pub used: u64,
    pub free: u64,
}

pub struct SymbolIndex<'a> {
    symbols: Vec<&'a Symbol>,
}

impl MapModel {
    #[must_use]
    pub fn object_totals(&self, region: Option<&str>) -> Vec<ObjectTotal<'_>> {
        let mut totals = BTreeMap::<_, u64>::new();
        for part in &self.contributions {
            let part_region = part.region.as_deref();
            if region.is_some_and(|region| part_region != Some(region)) {
                continue;
            }
            let bytes = totals
                .entry((part.object_file.as_str(), part_region))
                .or_default();
            *bytes = bytes.saturating_add(part.size);
        }
        totals
            .into_iter()
            .map(|((object_file, region), bytes)| ObjectTotal {
                object_file,
                region,
                bytes,
            })
            .collect()
    }

    #[must_use]
    pub fn region_totals(&self) -> Vec<RegionTotal<'_>> {
        let mut totals = BTreeMap::<_, (u64, u64)>::new();
        for region in &self.regions {
            let (used, free) = totals.entry(region.name.as_str()).or_default();
            *used = used.saturating_add(region.used);
            *free = free.saturating_add(region.free);
        }
        totals
            .into_iter()
            .map(|(region, (used, free))| RegionTotal { region, used, free })
            .collect()
    }

    #[must_use]
    pub fn symbol_index(&self) -> SymbolIndex<'_> {
        let mut symbols: Vec<_> = self.symbols.iter().collect();
        symbols.sort_unstable_by(|a, b| {
            (a.address_space.as_deref(), a.address, &a.name).cmp(&(
                b.address_space.as_deref(),
                b.address,
                &b.name,
            ))
        });
        SymbolIndex { symbols }
    }
}

impl<'a> SymbolIndex<'a> {
    #[must_use]
    pub fn symbol_at(&self, address: u64, space: Option<&str>) -> Option<&'a Symbol> {
        let at = self.symbols.partition_point(|symbol| {
            (symbol.address_space.as_deref(), symbol.address) <= (space, address)
        });
        let symbol = *self.symbols.get(at.checked_sub(1)?)?;
        if symbol.address_space.as_deref() != space {
            return None;
        }
        (symbol.address == address
            || symbol
                .size
                .is_none_or(|size| address - symbol.address < size))
        .then_some(symbol)
    }
}
