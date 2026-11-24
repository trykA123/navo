//! Path lookups for one watcher batch in walk mode: a location resolves relative to the
//! configured root, names compare the way `Location::same_place` compares paths, and each
//! parent's name map is built once per batch and kept current as entries change. Maps hold
//! name hashes, not names, so a 60k-entry folder costs about 1 MiB while mapped.

use std::collections::HashMap;
use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
use std::time::Instant;

use navo_core::{Location, ProviderPath, WindowsFoldKey};

use super::Clock;
use crate::table::{Flags, NameTable};

/// How many children are mapped between two deadline checks.
const CHECK_EVERY: usize = 256;

/// The batch's lock budget ran out; the caller reconciles the change outside the lock.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Expired;

/// A child name as `same_place` compares it below the root.
#[derive(Debug, PartialEq, Eq, Hash)]
enum Key {
    /// A local Windows-style root: Windows upcase equality.
    Folded(WindowsFoldKey),
    /// Any other root: exact names.
    Exact(String),
}

/// Name-to-id lookups shared by every change of one batch.
pub(super) struct BatchLookup {
    root: Location,
    folds: bool,
    /// Parent id to (name hash to child id); a hash collision falls back to a scan.
    children: HashMap<u32, HashMap<u64, u32>>,
    deadline: Option<(Clock, Instant)>,
    expired: bool,
    /// Children read while building maps, so tests can bound the work.
    #[cfg(test)]
    pub(super) visits: usize,
}

impl BatchLookup {
    /// A lookup below `root` that never expires.
    pub(super) fn new(root: &Location) -> Self {
        Self {
            folds: root.folds_case(),
            root: root.clone(),
            children: HashMap::new(),
            deadline: None,
            expired: false,
            #[cfg(test)]
            visits: 0,
        }
    }

    /// A lookup that fails with `Expired` once `clock` passes `budget` from now.
    pub(super) fn until(root: &Location, clock: &Clock, budget: std::time::Duration) -> Self {
        let mut lookup = Self::new(root);
        lookup.deadline = Some((clock.clone(), clock() + budget));
        lookup
    }

    /// Drops the deadline, for lookups made after the write lock is released.
    pub(super) fn unbounded(&mut self) {
        self.deadline = None;
        self.expired = false;
    }

    /// The live entry at `location`, or `None` when it is outside the root or not indexed.
    pub(super) fn entry(
        &mut self,
        table: &NameTable,
        location: &Location,
    ) -> Result<Option<u32>, Expired> {
        let Some(names) = self.relative(location) else {
            return Ok(None);
        };
        let mut id = 0;
        for (depth, name) in names.iter().rev().enumerate() {
            if depth > 0 && !is_folder(table, id) {
                return Ok(None);
            }
            let Some(child) = self.child(table, id, name)? else {
                return Ok(None);
            };
            id = child;
        }
        Ok(table.name(id).map(|_| id))
    }

    /// The live folder at `location`.
    pub(super) fn folder(
        &mut self,
        table: &NameTable,
        location: &Location,
    ) -> Result<Option<u32>, Expired> {
        Ok(self
            .entry(table, location)?
            .filter(|&id| is_folder(table, id)))
    }

    /// Records a pushed entry in its parent's map, when that map exists.
    pub(super) fn pushed(&mut self, table: &NameTable, parent: u32, id: u32, name: &str) {
        let hash = self.hash(name);
        let Some(children) = self.children.get_mut(&parent) else {
            return;
        };
        let held = children
            .get(&hash)
            .is_some_and(|&old| old != id && table.parent(old) == Some(parent));
        if !held {
            children.insert(hash, id);
        }
    }

    /// Forgets `id` before the caller removes it, so its name maps to nothing.
    pub(super) fn removing(&mut self, table: &NameTable, id: u32) {
        let (Some(parent), Some(name)) = (table.parent(id), table.name(id)) else {
            return;
        };
        let hash = self.hash(name);
        if let Some(children) = self.children.get_mut(&parent)
            && children.get(&hash) == Some(&id)
        {
            children.remove(&hash);
        }
        self.children.remove(&id);
    }

    /// Child names from `location` up to the root, innermost first; `None` outside the root.
    fn relative(&self, location: &Location) -> Option<Vec<String>> {
        let mut names = Vec::new();
        let mut at = location.clone();
        while !self.root.same_place(&at) {
            let (parent, name) = at.parent_and_name();
            names.push(name?.to_text());
            at = parent?;
        }
        Some(names)
    }

    fn child(
        &mut self,
        table: &NameTable,
        parent: u32,
        name: &str,
    ) -> Result<Option<u32>, Expired> {
        self.check()?;
        if !self.children.contains_key(&parent) {
            let map = self.map(table, parent)?;
            self.children.insert(parent, map);
        }
        let key = self.key(name);
        let hash = hash_of(&key);
        let Some(&id) = self.children.get(&parent).and_then(|map| map.get(&hash)) else {
            return Ok(None);
        };
        if table.parent(id) != Some(parent) {
            return Ok(None);
        }
        if table.name(id).is_some_and(|found| self.key(found) == key) {
            return Ok(Some(id));
        }
        Ok(self.scan(table, parent, &key))
    }

    /// The child named `key`, read linearly; only for a hash shared by two names.
    fn scan(&self, table: &NameTable, parent: u32, key: &Key) -> Option<u32> {
        table
            .children(parent)
            .find(|&child| table.name(child).is_some_and(|name| self.key(name) == *key))
    }

    /// Maps every live child of `parent` by name hash; the first of equal hashes wins.
    fn map(&mut self, table: &NameTable, parent: u32) -> Result<HashMap<u64, u32>, Expired> {
        let mut map = HashMap::new();
        for (at, child) in table.children(parent).enumerate() {
            if at % CHECK_EVERY == CHECK_EVERY - 1 {
                self.check()?;
            }
            #[cfg(test)]
            {
                self.visits += 1;
            }
            if let Some(name) = table.name(child) {
                map.entry(self.hash(name)).or_insert(child);
            }
        }
        Ok(map)
    }

    fn key(&self, name: &str) -> Key {
        if self.folds {
            Key::Folded(ProviderPath::from_text(name).windows_fold_key())
        } else {
            Key::Exact(name.to_owned())
        }
    }

    fn hash(&self, name: &str) -> u64 {
        hash_of(&self.key(name))
    }

    /// Fails once the deadline has passed, and keeps failing for the rest of the batch.
    fn check(&mut self) -> Result<(), Expired> {
        if !self.expired
            && let Some((clock, deadline)) = &self.deadline
        {
            self.expired = clock() >= *deadline;
        }
        if self.expired { Err(Expired) } else { Ok(()) }
    }
}

fn hash_of(key: &Key) -> u64 {
    BuildHasherDefault::<DefaultHasher>::default().hash_one(key)
}

fn is_folder(table: &NameTable, id: u32) -> bool {
    table
        .flags(id)
        .is_some_and(|flags| flags.contains(Flags::FOLDER))
}

#[cfg(test)]
mod tests {
    use navo_fs::FakeFs;

    use super::*;
    use crate::table::NO_PARENT;

    /// The walker stores the whole root path in slot 0; a folder below it resolves with
    /// Windows path equality, where `ασ` and `ΑΣ` (which lowercases to `ας`) are one name.
    #[test]
    fn a_folder_resolves_below_a_windows_root_with_windows_case_rules() {
        let mut table = NameTable::new();
        let root = table.push(NO_PARENT, r"C:\Work", Flags::FOLDER).unwrap();
        let docs = table.push(root, "Docs", Flags::FOLDER).unwrap();
        let sigma = table.push(docs, "ΑΣ", Flags::FOLDER).unwrap();
        let mut lookup = BatchLookup::new(&Location::new(r"C:\Work").unwrap());
        let location = Location::new(r"c:\work\DOCS\ασ").unwrap();
        assert_eq!(lookup.folder(&table, &location), Ok(Some(sigma)));
        let outside = Location::new(r"C:\Other\Docs").unwrap();
        assert_eq!(lookup.folder(&table, &outside), Ok(None));
    }

    /// Two names sharing a hash still resolve: the mapped child's name is checked, and a
    /// mismatch falls back to reading the folder.
    #[test]
    fn a_shared_hash_falls_back_to_the_folder_scan() {
        let fs = FakeFs::new();
        let mut table = NameTable::new();
        let root = table.push(NO_PARENT, "tree", Flags::FOLDER).unwrap();
        let a = table.push(root, "a", Flags::NONE).unwrap();
        let b = table.push(root, "b", Flags::NONE).unwrap();
        let mut lookup = BatchLookup::new(&fs.location("tree"));
        assert_eq!(lookup.entry(&table, &fs.location("tree/a")), Ok(Some(a)));
        let shared = lookup.hash("b");
        lookup.children.get_mut(&root).unwrap().insert(shared, a);
        assert_eq!(lookup.entry(&table, &fs.location("tree/b")), Ok(Some(b)));
        assert_eq!(lookup.entry(&table, &fs.location("tree/c")), Ok(None));
    }

    /// 200 changes in a 20,000-entry folder read its children once, and the map follows
    /// every removal and push of the batch.
    #[test]
    fn a_flat_folder_is_mapped_once_per_batch_and_kept_current() {
        const FILES: usize = 20_000;
        let fs = FakeFs::new();
        let mut table = NameTable::new();
        let root = table.push(NO_PARENT, "tree", Flags::FOLDER).unwrap();
        let big = table.push(root, "big", Flags::FOLDER).unwrap();
        for at in 0..FILES {
            table
                .push(big, &format!("file-{at:05}.txt"), Flags::NONE)
                .unwrap();
        }
        let mut lookup = BatchLookup::new(&fs.location("tree"));
        for at in (0..FILES).step_by(100) {
            let name = format!("file-{at:05}.txt");
            let location = fs.location(&format!("tree/big/{name}"));
            let old = lookup.entry(&table, &location).unwrap().unwrap();
            lookup.removing(&table, old);
            table.remove(old);
            assert_eq!(lookup.entry(&table, &location), Ok(None));
            let new = table.push(big, &name, Flags::NONE).unwrap();
            lookup.pushed(&table, big, new, &name);
            assert_eq!(lookup.entry(&table, &location), Ok(Some(new)));
        }
        assert_eq!(
            lookup.visits,
            FILES + 1,
            "children read beyond one map per folder"
        );
    }
}
