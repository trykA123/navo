use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::{Flags, MAX_DEPTH, NameTable};

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("folder lookup exceeded its deadline")]
pub struct FolderLookupExpired;

/// Finds folders by path in one table; a parent's first visit walks its children without a
/// map, and a second visit indexes its folder children by lowercased name (a cache).
pub struct FolderLookup<'a> {
    table: &'a NameTable,
    walked: Vec<u32>,
    children: HashMap<u32, HashMap<String, u32>>,
}

impl NameTable {
    pub fn find_folder(&self, path: &str) -> Option<u32> {
        self.folder_lookup()
            .find_folder(path, Instant::now() + Duration::from_millis(20))
            .ok()
            .flatten()
    }

    pub fn folder_lookup(&self) -> FolderLookup<'_> {
        FolderLookup {
            table: self,
            walked: Vec::new(),
            children: HashMap::new(),
        }
    }
}

impl FolderLookup<'_> {
    pub fn find_folder(
        &mut self,
        path: &str,
        deadline: Instant,
    ) -> Result<Option<u32>, FolderLookupExpired> {
        self.find_before(path, &mut || Instant::now() >= deadline)
    }

    fn find_before(
        &mut self,
        path: &str,
        expired: &mut impl FnMut() -> bool,
    ) -> Result<Option<u32>, FolderLookupExpired> {
        let mut parts = path.trim_end_matches(['/', '\\']).split(['/', '\\']);
        if expired() {
            return Err(FolderLookupExpired);
        }
        let root = parts.next().map(str::to_lowercase).unwrap_or_default();
        if !self.table.name(0).is_some_and(|name| folds_to(name, &root)) {
            return Ok(None);
        }
        let mut id = 0;
        for (depth, part) in parts.enumerate() {
            if expired() || depth >= MAX_DEPTH - 1 {
                return Err(FolderLookupExpired);
            }
            let Some(child) = self.child(id, part, expired)? else {
                return Ok(None);
            };
            id = child;
        }
        if expired() {
            return Err(FolderLookupExpired);
        }
        Ok(self.table.totals(id).map(|_| id))
    }

    /// Finds the folder child `name` of `parent`, walking on the first visit and indexing the
    /// children on the second; fails when `expired` turns true mid-walk (nothing is cached then).
    fn child(
        &mut self,
        parent: u32,
        name: &str,
        expired: &mut impl FnMut() -> bool,
    ) -> Result<Option<u32>, FolderLookupExpired> {
        let folded = name.to_lowercase();
        if let Some(children) = self.children.get(&parent) {
            return Ok(children.get(&folded).copied());
        }
        if self.walked.contains(&parent) {
            let children = index_children(self.table, parent, expired)?;
            return Ok(self
                .children
                .entry(parent)
                .or_insert(children)
                .get(&folded)
                .copied());
        }
        let found = find_child(self.table, parent, &folded, expired)?;
        self.walked.push(parent);
        Ok(found)
    }
}

/// Returns the first folder child of `parent` whose name folds to `folded`, the one the
/// indexed map keeps; fails when `expired` turns true first.
fn find_child(
    table: &NameTable,
    parent: u32,
    folded: &str,
    expired: &mut impl FnMut() -> bool,
) -> Result<Option<u32>, FolderLookupExpired> {
    for child in table.children.of(parent) {
        if expired() {
            return Err(FolderLookupExpired);
        }
        if table
            .flags(child)
            .is_some_and(|flags| flags.contains(Flags::FOLDER))
            && table.name(child).is_some_and(|name| folds_to(name, folded))
        {
            return Ok(Some(child));
        }
    }
    Ok(None)
}

/// True when `name` lowercases to `folded` (already lowercase) like `str::to_lowercase`; an
/// ASCII name compares without allocating, since it can only lowercase to ASCII.
fn folds_to(name: &str, folded: &str) -> bool {
    if name.is_ascii() {
        folded.is_ascii() && name.eq_ignore_ascii_case(folded)
    } else {
        name.to_lowercase() == folded
    }
}

fn index_children(
    table: &NameTable,
    parent: u32,
    expired: &mut impl FnMut() -> bool,
) -> Result<HashMap<String, u32>, FolderLookupExpired> {
    let mut children = HashMap::new();
    for child in table.children.of(parent) {
        if expired() {
            return Err(FolderLookupExpired);
        }
        if table
            .flags(child)
            .is_some_and(|flags| flags.contains(Flags::FOLDER))
            && let Some(name) = table.name(child)
        {
            children.entry(name.to_lowercase()).or_insert(child);
        }
    }
    Ok(children)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::NO_PARENT;

    fn wide_table() -> (NameTable, u32) {
        let mut table = NameTable::new();
        table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
        let needle = table.push(0, "Ä", Flags::FOLDER).unwrap();
        for _ in 0..1000 {
            table.push(0, "file", Flags::NONE).unwrap();
        }
        (table, needle)
    }

    #[test]
    fn deadline_interrupts_a_large_child_scan_without_caching_partial_results() {
        let (table, needle) = wide_table();
        let mut lookup = table.folder_lookup();
        let mut checks = 0;
        let result = lookup.find_before("C:/ä", &mut || {
            checks += 1;
            checks == 5
        });
        assert_eq!(result, Err(FolderLookupExpired));
        assert_eq!(checks, 5);
        assert!(lookup.children.is_empty());
        assert_eq!(
            lookup.find_before("C:/ä", &mut || false).unwrap(),
            Some(needle)
        );
    }

    #[test]
    fn repeated_paths_reuse_the_folded_child_name_lookup() {
        let (table, needle) = wide_table();
        let mut lookup = table.folder_lookup();
        for _ in 0..2 {
            assert_eq!(
                lookup.find_before("C:/ä", &mut || false).unwrap(),
                Some(needle)
            );
        }
        let mut checks = 0;
        for _ in 0..64 {
            assert_eq!(
                lookup
                    .find_before("c:/Ä", &mut || {
                        checks += 1;
                        false
                    })
                    .unwrap(),
                Some(needle)
            );
        }
        assert_eq!(checks, 64 * 3);
        assert_eq!(lookup.children[&0].len(), 1);
    }

    #[test]
    fn expired_deadlines_also_reject_root_and_cached_paths() {
        let (table, _) = wide_table();
        let mut lookup = table.folder_lookup();
        let deadline = Instant::now();
        assert_eq!(lookup.find_folder("C:", deadline), Err(FolderLookupExpired));
        assert_eq!(
            lookup.find_folder("C:/ä", deadline),
            Err(FolderLookupExpired)
        );
    }

    #[test]
    fn a_single_lookup_walks_without_a_map_and_folds_like_the_map() {
        let mut table = NameTable::new();
        table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
        let kelvin = table.push(0, "\u{212A}", Flags::FOLDER).unwrap();
        let sigma = table.push(0, "ΟΔΟΣ", Flags::FOLDER).unwrap();
        table.push(0, "plain", Flags::NONE).unwrap();
        let plain = table.push(0, "PLAIN", Flags::FOLDER).unwrap();
        let cases = [
            ("C:/k", Some(kelvin)),
            ("c:/οδος", Some(sigma)),
            ("C:/plain", Some(plain)),
        ];
        for (path, expected) in cases {
            let mut lookup = table.folder_lookup();
            assert_eq!(lookup.find_before(path, &mut || false).unwrap(), expected);
            assert!(lookup.children.is_empty(), "one visit must not build a map");
            assert_eq!(lookup.find_before(path, &mut || false).unwrap(), expected);
            assert_eq!(
                lookup.children.len(),
                1,
                "a second visit indexes the parent"
            );
        }
    }
}
