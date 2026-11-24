//! File identities for F5/F6 from Find results. A mapped drive and its share, a SUBST drive, a
//! hard link or a volume mount point give one entry two paths but one volume serial and file
//! ID. Paths are only the fast first check; identities decide whether a write hits a source.

use std::collections::{HashMap, HashSet};

use navo_core::{FileIdentity, FsErrorKind, Location};
use navo_fs::ProviderOps;

/// An identity as a hash key: its volume serial and file ID. Kinds are not compared, so two
/// kinds of one ID count as one entry (the safe side).
type Key = (u64, [u8; 16]);

fn key(identity: FileIdentity) -> Key {
    (identity.volume, identity.file)
}

/// What one identity read found.
#[derive(Debug, Clone, Copy)]
enum Seen {
    Missing,
    Known(FileIdentity),
    /// The entry exists on a volume without stable file IDs (FAT, some shares).
    NoIds,
    /// The read failed for another reason.
    Failed,
}

/// Why a location fails the identity check.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Clash {
    /// It is a selected row, or a folder that holds one.
    Source,
    /// It lies inside the selected folder named here.
    Inside(Location),
    /// It exists, but Navo could not read which entry it is.
    Unknown(Location),
}

/// One selected row for the identity check: its resolved place, its identity (`None` on a
/// volume without stable file IDs) and whether it is a folder.
pub(super) struct Row<'r> {
    pub real: &'r Location,
    pub identity: Option<FileIdentity>,
    pub folder: bool,
}

/// The selected rows by identity, with a cache of every identity read.
pub(super) struct Identities<'a> {
    provider: &'a dyn ProviderOps,
    seen: HashMap<Location, Seen>,
    rows: HashSet<Key>,
    folders: HashMap<Key, Location>,
    /// A row, or a folder row, on a volume without stable IDs: an entry there with no ID may be it.
    rows_without_ids: bool,
    folders_without_ids: bool,
    /// The rows' parent folders, read into `holders` on first need.
    parents: Vec<Location>,
    holders: Option<Result<HashSet<Key>, Location>>,
    /// Folders already proven to lie outside every selected folder.
    clear: HashSet<Location>,
}

impl<'a> Identities<'a> {
    pub(super) fn new(provider: &'a dyn ProviderOps, rows: &[Row<'_>]) -> Self {
        let folders = || rows.iter().filter(|row| row.folder);
        Self {
            provider,
            seen: HashMap::new(),
            rows: rows
                .iter()
                .filter_map(|row| row.identity.map(key))
                .collect(),
            folders: folders()
                .filter_map(|row| Some((key(row.identity?), row.real.clone())))
                .collect(),
            rows_without_ids: rows.iter().any(|row| row.identity.is_none()),
            folders_without_ids: folders().any(|row| row.identity.is_none()),
            parents: rows.iter().filter_map(|row| row.real.parent()).collect(),
            holders: None,
            clear: HashSet::new(),
        }
    }

    /// Refuses `folder` when it, or a folder above it, is a selected folder by identity.
    /// Reads nothing when no row is a folder.
    pub(super) fn inside(&mut self, folder: &Location) -> Result<(), Clash> {
        if self.folders.is_empty() && !self.folders_without_ids {
            return Ok(());
        }
        let mut walked = Vec::new();
        for at in std::iter::successors(Some(folder.clone()), Location::parent) {
            if self.clear.contains(&at) {
                break;
            }
            self.folder_clash(&at)?;
            walked.push(at);
        }
        self.clear.extend(walked);
        Ok(())
    }

    /// Refuses the folder `at` when it is a selected folder, or exists but cannot be told apart.
    fn folder_clash(&mut self, at: &Location) -> Result<(), Clash> {
        match self.read(at) {
            Seen::Failed => Err(Clash::Unknown(at.clone())),
            Seen::NoIds if self.folders_without_ids => Err(Clash::Unknown(at.clone())),
            Seen::Known(identity) => self
                .folders
                .get(&key(identity))
                .map_or(Ok(()), |row| Err(Clash::Inside(row.clone()))),
            Seen::Missing | Seen::NoIds => Ok(()),
        }
    }

    /// Refuses a write onto `target` when the entry there is a selected row, or a folder that
    /// holds one, by identity; and when it exists but its identity cannot be read.
    pub(super) fn write_onto(&mut self, target: &Location) -> Result<(), Clash> {
        let identity = match self.read(target) {
            Seen::Missing => return Ok(()),
            Seen::NoIds if !self.rows_without_ids => return Ok(()),
            Seen::NoIds | Seen::Failed => return Err(Clash::Unknown(target.clone())),
            Seen::Known(identity) => identity,
        };
        if self.rows.contains(&key(identity)) || self.holds(identity)? {
            return Err(Clash::Source);
        }
        Ok(())
    }

    /// True when `identity` is a folder above a selected row; those folders are read once.
    fn holds(&mut self, identity: FileIdentity) -> Result<bool, Clash> {
        let holders = match self.holders.take() {
            Some(holders) => holders,
            None => self.read_holders(),
        };
        let holds = match &holders {
            Ok(found) => Ok(found.contains(&key(identity))),
            Err(folder) => Err(Clash::Unknown(folder.clone())),
        };
        self.holders = Some(holders);
        holds
    }

    fn read_holders(&mut self) -> Result<HashSet<Key>, Location> {
        let folders: Vec<Location> = self
            .parents
            .iter()
            .flat_map(|parent| std::iter::successors(Some(parent.clone()), Location::parent))
            .collect();
        let mut found = HashSet::new();
        for folder in folders {
            match self.read(&folder) {
                Seen::Known(identity) => {
                    found.insert(key(identity));
                }
                Seen::Failed => return Err(folder),
                Seen::Missing | Seen::NoIds => {}
            }
        }
        Ok(found)
    }

    fn read(&mut self, at: &Location) -> Seen {
        if let Some(seen) = self.seen.get(at) {
            return *seen;
        }
        let seen = match self.provider.file_identity(at) {
            Ok(Some(identity)) => Seen::Known(identity),
            Ok(None) => Seen::NoIds,
            Err(error) if error.kind == FsErrorKind::NotFound => Seen::Missing,
            Err(_) => Seen::Failed,
        };
        self.seen.insert(at.clone(), seen);
        seen
    }
}

#[cfg(test)]
mod tests {
    use navo_core::ProviderPath;
    use navo_fs::LocalFs;

    use super::*;

    fn local(path: &std::path::Path) -> Location {
        let text = path.to_str().expect("a UTF-8 temp path");
        Location::local(ProviderPath::from_text(text)).expect("a location")
    }

    #[test]
    fn a_hard_link_to_a_selected_file_is_the_file() {
        let dir = tempfile::tempdir().expect("a temp folder");
        let (file, link, other) = (
            dir.path().join("a"),
            dir.path().join("b"),
            dir.path().join("c"),
        );
        std::fs::write(&file, b"one").expect("a file");
        std::fs::write(&other, b"one").expect("a file");
        std::fs::hard_link(&file, &link).expect("a hard link");
        let fs = LocalFs::new();
        let real = local(&file);
        let identity = fs.file_identity(&real).expect("an identity");
        let rows = [Row {
            real: &real,
            identity,
            folder: false,
        }];
        let mut identities = Identities::new(&fs, &rows);
        assert_eq!(identities.write_onto(&local(&link)), Err(Clash::Source));
        assert_eq!(identities.write_onto(&local(&other)), Ok(()));
        let missing = local(&dir.path().join("new"));
        assert_eq!(identities.write_onto(&missing), Ok(()));
    }
}
