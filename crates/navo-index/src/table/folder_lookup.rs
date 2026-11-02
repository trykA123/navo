use std::collections::{HashMap, hash_map::Entry};
use std::time::{Duration, Instant};

use super::{Flags, MAX_DEPTH, NameTable};

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("folder lookup exceeded its deadline")]
pub struct FolderLookupExpired;

pub struct FolderLookup<'a> {
    table: &'a NameTable,
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
        if self.table.name(0).map(str::to_lowercase) != parts.next().map(str::to_lowercase) {
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

    fn child(
        &mut self,
        parent: u32,
        name: &str,
        expired: &mut impl FnMut() -> bool,
    ) -> Result<Option<u32>, FolderLookupExpired> {
        let children = match self.children.entry(parent) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(index_children(self.table, parent, expired)?),
        };
        Ok(children.get(&name.to_lowercase()).copied())
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
        assert_eq!(
            lookup.find_before("C:/ä", &mut || false).unwrap(),
            Some(needle)
        );
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
}
