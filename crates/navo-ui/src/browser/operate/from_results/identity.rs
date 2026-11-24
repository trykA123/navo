//! File identities for F5/F6 from Find results. A mapped drive and its share, a SUBST drive, a
//! hard link or a volume mount point give one entry two paths but one volume serial and file
//! ID. Paths are only the fast first check; identities decide whether a write hits a source.
//! On a volume without file IDs (FAT, exFAT), another volume serial clears an entry and the same
//! serial falls back to the resolved path. Known limit: there, a SUBST drive or a second mapping
//! of one folder gives one entry two paths that no check can join.

use std::collections::{HashMap, HashSet};

use navo_core::{FileIdentity, FsErrorKind, Location};
use navo_fs::ProviderOps;

use super::alias::{Place, place};

/// A volume serial cut to 32 bits: `FILE_ID_INFO` reports 64 bits, `dwVolumeSerialNumber` 32,
/// so both forms of one volume match. Extra matches only refuse more.
type Serial = u32;

fn serial(volume: u64) -> Serial {
    volume as u32
}

/// An identity as a hash key: its volume serial and file ID. Kinds are not compared, so two
/// kinds of one ID count as one entry (the safe side).
type Key = (Serial, [u8; 16]);

fn key(identity: FileIdentity) -> Key {
    (serial(identity.volume), identity.file)
}

/// What one identity read found.
#[derive(Debug, Clone, Copy)]
enum Seen {
    Missing,
    Known(FileIdentity),
    /// The entry exists on a volume without stable file IDs (FAT, some shares); holds that
    /// volume's serial, `None` when unreadable.
    NoIds(Option<Serial>),
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

/// The selected rows without file IDs, by resolved place and volume serial.
#[derive(Default)]
struct Bare {
    /// Each such folder row's place: its volume serials and locations.
    folders: HashMap<Place, Vec<(Serial, Location)>>,
    /// Each such row's place and every folder above it: the rows' volume serials.
    held: HashMap<Place, HashSet<Serial>>,
    /// The volume serial of one such row could not be read.
    unknown: bool,
}

impl Bare {
    fn new(provider: &dyn ProviderOps, rows: &[Row<'_>]) -> Self {
        let mut bare = Self::default();
        for row in rows.iter().filter(|row| row.identity.is_none()) {
            let Some(volume) = provider.volume_serial(row.real).ok().flatten() else {
                bare.unknown = true;
                continue;
            };
            let volume = serial(volume);
            if row.folder {
                let at = bare.folders.entry(place(row.real)).or_default();
                at.push((volume, row.real.clone()));
            }
            for folder in std::iter::successors(Some(row.real.clone()), Location::parent) {
                bare.held.entry(place(&folder)).or_default().insert(volume);
            }
        }
        bare
    }
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
    bare: Bare,
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
            bare: Bare::new(provider, rows),
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
    /// Without IDs, a folder row on the same volume at the same resolved path is a match.
    fn folder_clash(&mut self, at: &Location) -> Result<(), Clash> {
        match self.read(at) {
            Seen::Failed => Err(Clash::Unknown(at.clone())),
            Seen::NoIds(volume) if self.folders_without_ids => {
                let volume = self.bare_volume(at, volume)?;
                let mut rows = self.bare.folders.get(&place(at)).into_iter().flatten();
                rows.find(|(on, _)| *on == volume)
                    .map_or(Ok(()), |(_, row)| Err(Clash::Inside(row.clone())))
            }
            Seen::Known(identity) => self
                .folders
                .get(&key(identity))
                .map_or(Ok(()), |row| Err(Clash::Inside(row.clone()))),
            Seen::Missing | Seen::NoIds(_) => Ok(()),
        }
    }

    /// Refuses a write onto `target` when the entry there is a selected row, or a folder that
    /// holds one, by identity; and when it exists but its identity cannot be read. Without IDs,
    /// the volume serial and then the resolved path decide.
    pub(super) fn write_onto(&mut self, target: &Location) -> Result<(), Clash> {
        let identity = match self.read(target) {
            Seen::Missing => return Ok(()),
            Seen::NoIds(_) if !self.rows_without_ids => return Ok(()),
            Seen::NoIds(volume) => {
                let volume = self.bare_volume(target, volume)?;
                let held = self.bare.held.get(&place(target));
                return match held.is_some_and(|on| on.contains(&volume)) {
                    true => Err(Clash::Source),
                    false => Ok(()),
                };
            }
            Seen::Failed => return Err(Clash::Unknown(target.clone())),
            Seen::Known(identity) => identity,
        };
        if self.rows.contains(&key(identity)) || self.holds(identity)? {
            return Err(Clash::Source);
        }
        Ok(())
    }

    /// The volume serial of the ID-less entry `at`; refuses it when that serial, or the serial
    /// of any ID-less row, could not be read.
    fn bare_volume(&self, at: &Location, volume: Option<Serial>) -> Result<Serial, Clash> {
        volume
            .filter(|_| !self.bare.unknown)
            .ok_or_else(|| Clash::Unknown(at.clone()))
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
                Seen::Missing | Seen::NoIds(_) => {}
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
            Ok(None) => Seen::NoIds(self.provider.volume_serial(at).ok().flatten().map(serial)),
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

    #[test]
    fn a_64_bit_and_a_32_bit_serial_of_one_volume_match() {
        let fs = navo_fs::FakeFs::new();
        fs.add_file("v/a.c", 1);
        fs.set_volume_serial("v", Some(0x9abc_def0));
        let real = fs.location("v/a.c");
        let found = fs.file_identity(&real).expect("an identity");
        let identity = found.map(|found| FileIdentity {
            volume: 0x1234_5678_9abc_def0,
            ..found
        });
        let rows = [Row {
            real: &real,
            identity,
            folder: false,
        }];
        let mut identities = Identities::new(&fs, &rows);
        assert_eq!(identities.write_onto(&real), Err(Clash::Source));
    }

    #[test]
    fn entries_without_ids_compare_volume_serials_then_paths() {
        let fs = navo_fs::FakeFs::new();
        fs.add_file("w/src/can/a.c", 1);
        fs.add_file("w/top.c", 1);
        fs.add_file("w/sorted/top.c", 1);
        fs.add_file("f/top.c", 1);
        fs.add_folder("g");
        for (at, serial) in [("w", Some(5)), ("f", Some(6)), ("g", None)] {
            fs.set_volume_serial(at, serial);
            fs.set_without_ids(at);
        }
        let (src, top) = (fs.location("w/src"), fs.location("w/top.c"));
        let rows = [
            Row {
                real: &src,
                identity: None,
                folder: true,
            },
            Row {
                real: &top,
                identity: None,
                folder: false,
            },
        ];
        let mut identities = Identities::new(&fs, &rows);
        let inside = Err(Clash::Inside(src.clone()));
        assert_eq!(identities.inside(&fs.location("w/src/can")), inside);
        assert_eq!(identities.inside(&fs.location("w/sorted")), Ok(()));
        assert_eq!(identities.inside(&fs.location("f")), Ok(()));
        let mut onto = |at: &str| identities.write_onto(&fs.location(at));
        assert_eq!(onto("w/top.c"), Err(Clash::Source));
        assert_eq!(onto("w"), Err(Clash::Source));
        assert_eq!(onto("w/sorted/top.c"), Ok(()));
        assert_eq!(onto("f/top.c"), Ok(()));
        assert_eq!(onto("g"), Err(Clash::Unknown(fs.location("g"))));
    }
}
