use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

use rusty_core::brand;
use rusty_store::StoreDirs;

use super::fs::MigrationFs;

pub(super) const DETACHED: &str = ".navo-migrating";
pub(super) const RETIRED: &str = ".migrated-to-navo";
pub(super) const CREATED: &str = ".created-during-migration";
pub(super) const CORRUPT: &str = ".corrupt";
const MAX_SUFFIX: u32 = 1000;

pub(super) struct Entry {
    old: String,
    new: String,
    pub(super) detach: bool,
    database: bool,
}

pub(super) struct RestoreSet<'a> {
    pub(super) entries: Vec<&'a Entry>,
    pub(super) companions: Vec<PathBuf>,
}

pub(super) struct Root {
    pub(super) old: PathBuf,
    pub(super) new: PathBuf,
    pub(super) entries: Vec<Entry>,
}

pub(super) struct Plan {
    pub(super) roots: Vec<Root>,
    pub(super) journal: PathBuf,
    pub(super) marker: PathBuf,
}

impl Plan {
    pub(super) fn new(old: &StoreDirs, new: &StoreDirs) -> io::Result<Self> {
        let parent = new.data.parent().filter(|p| !p.as_os_str().is_empty());
        let parent = parent.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} has no parent folder", new.data.display()),
            )
        })?;
        let mut roots = vec![Root {
            old: old.config.clone(),
            new: new.config.clone(),
            entries: config_entries(),
        }];
        if old.data == old.config && new.data == new.config {
            roots[0].entries.extend(data_entries());
        } else {
            roots.push(Root {
                old: old.data.clone(),
                new: new.data.clone(),
                entries: data_entries(),
            });
        }
        Ok(Self {
            roots,
            journal: parent.join(format!("{}-migration.json", brand::NAME)),
            marker: new
                .data
                .join(format!("migrated-from-{}.json", brand::LEGACY_SLUG)),
        })
    }
}

impl Root {
    pub(super) fn in_place(&self) -> bool {
        self.old == self.new
    }

    pub(super) fn staging(&self) -> PathBuf {
        if self.in_place() {
            return self.new.join(format!("{}.staging", brand::NAME));
        }
        with_suffix(&self.new, ".staging")
    }

    pub(super) fn moves(&self, entry: &Entry) -> bool {
        entry.detach || !self.in_place() || entry.old != entry.new
    }

    pub(super) fn moving(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|entry| self.moves(entry))
    }

    pub(super) fn renamed(&self) -> impl Iterator<Item = &Entry> {
        self.entries
            .iter()
            .filter(|entry| !self.in_place() || entry.old != entry.new)
    }

    pub(super) fn original(&self, entry: &Entry) -> PathBuf {
        self.old.join(&entry.old)
    }

    pub(super) fn detached(&self, entry: &Entry) -> PathBuf {
        self.old.join(format!("{}{DETACHED}", entry.old))
    }

    pub(super) fn retired(&self, entry: &Entry) -> PathBuf {
        self.old.join(format!("{}{RETIRED}", entry.old))
    }

    pub(super) fn source(&self, entry: &Entry) -> PathBuf {
        if entry.detach {
            self.detached(entry)
        } else {
            self.original(entry)
        }
    }

    pub(super) fn target(&self, entry: &Entry) -> PathBuf {
        self.new.join(&entry.new)
    }

    pub(super) fn staged(&self, entry: &Entry) -> PathBuf {
        self.staging().join(&entry.new)
    }

    pub(super) fn restore_sets(&self) -> Vec<RestoreSet<'_>> {
        let detached = self.entries.iter().filter(|entry| entry.detach);
        let (database, single): (Vec<_>, Vec<_>) = detached.partition(|entry| entry.database);
        let companions = if database.is_empty() {
            Vec::new()
        } else {
            vec![self.old.join(format!("{}.db-shm", brand::LEGACY_SLUG))]
        };
        let single = single.into_iter().map(|entry| RestoreSet {
            entries: vec![entry],
            companions: Vec::new(),
        });
        std::iter::once(RestoreSet {
            entries: database,
            companions,
        })
        .chain(single)
        .collect()
    }

    pub(super) fn is_target(&self, name: &OsStr) -> bool {
        self.moving().any(|entry| name == OsStr::new(&entry.new))
    }

    pub(super) fn retired_root(&self) -> PathBuf {
        with_suffix(&self.old, RETIRED)
    }
}

pub(super) fn free_name(fs: &impl MigrationFs, wanted: &Path) -> io::Result<PathBuf> {
    let mut names = free_names(fs, &[wanted.to_owned()])?;
    Ok(names.pop().unwrap_or_else(|| wanted.to_owned()))
}

pub(super) fn free_names(fs: &impl MigrationFs, wanted: &[PathBuf]) -> io::Result<Vec<PathBuf>> {
    let numbered = |n: u32| -> Vec<PathBuf> {
        wanted
            .iter()
            .map(|path| match n {
                1 => path.clone(),
                n => with_suffix(path, &format!("-{n}")),
            })
            .collect()
    };
    (1..MAX_SUFFIX)
        .map(numbered)
        .find(|names| !names.iter().any(|name| fs.exists(name)))
        .ok_or_else(|| {
            let first = wanted.first().map(|path| path.display().to_string());
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("no free name next to {}", first.unwrap_or_default()),
            )
        })
}

pub(super) fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

pub(super) fn is_rebuildable(name: &OsStr) -> bool {
    let lock = format!("{}.lock", brand::SLUG);
    ["logs", "index", "cache.db", "running", lock.as_str()]
        .iter()
        .any(|rebuilt| name == OsStr::new(rebuilt))
}

fn entry(old: String, new: String, detach: bool) -> Entry {
    Entry {
        old,
        new,
        detach,
        database: false,
    }
}

fn database(old: String, new: String) -> Entry {
    Entry {
        database: true,
        ..entry(old, new, true)
    }
}

fn config_entries() -> Vec<Entry> {
    let (old, new) = (brand::LEGACY_SLUG, brand::SLUG);
    vec![
        entry(format!("{old}.toml"), format!("{new}.toml"), false),
        entry(
            format!("{old}.toml.defaults-pending"),
            format!("{new}.toml.defaults-pending"),
            false,
        ),
        entry("keymap.toml".into(), "keymap.toml".into(), false),
        entry("rename.log".into(), "rename.log".into(), false),
        entry("rename.log.1".into(), "rename.log.1".into(), false),
    ]
}

fn data_entries() -> Vec<Entry> {
    let (old, new) = (brand::LEGACY_SLUG, brand::SLUG);
    vec![
        database(format!("{old}.db"), format!("{new}.db")),
        database(format!("{old}.db-wal"), format!("{new}.db-wal")),
        entry("session.bin".into(), "session.bin".into(), true),
    ]
}
