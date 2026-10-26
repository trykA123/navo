use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusty_store::StoreDirs;
use tempfile::TempDir;

use super::super::{Migration, MigrationFs, Outcome};
use super::fault_fs::{Fault, FaultFs};

pub(super) type Tree = BTreeMap<String, Option<Vec<u8>>>;

pub(super) const MARKER: &str = "migrated-from-ferrite.json";

pub(super) fn layouts() -> [fn() -> Fixture; 2] {
    [Fixture::user, Fixture::profile]
}

pub(super) fn write_count(make: fn() -> Fixture) -> usize {
    let fixture = make();
    let fs = FaultFs::new(Fault::None);
    assert!(matches!(fixture.run(&fs), Outcome::Done));
    fs.writes()
}

pub(super) fn crashed_where(
    make: fn() -> Fixture,
    found: impl Fn(&Fixture) -> bool,
) -> (Fixture, Tree) {
    for k in 0..write_count(make) {
        let fixture = make();
        let original = fixture.tree();
        let _ = fixture.run(&FaultFs::new(Fault::CrashAt(k)));
        if found(&fixture) {
            return (fixture, original);
        }
    }
    panic!("no process crash point reaches the wanted state");
}

pub(super) struct Fixture {
    pub(super) temp: TempDir,
    pub(super) old: StoreDirs,
    pub(super) new: StoreDirs,
}

impl Fixture {
    pub(super) fn user() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let old = StoreDirs {
            config: temp.path().join("roaming/Ferrite"),
            data: temp.path().join("local/Ferrite"),
        };
        let new = StoreDirs {
            config: temp.path().join("roaming/Navo"),
            data: temp.path().join("local/Navo"),
        };
        let fixture = Self { temp, old, new };
        fixture.seed();
        fixture
    }

    pub(super) fn profile() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let dirs = StoreDirs::in_profile(&temp.path().join("p"));
        let fixture = Self {
            temp,
            old: dirs.clone(),
            new: dirs,
        };
        fixture.seed();
        fixture
    }

    pub(super) fn in_place(&self) -> bool {
        self.old == self.new
    }

    pub(super) fn migration<'a, F: MigrationFs>(&self, fs: &'a F) -> Migration<'a, F> {
        if self.in_place() {
            Migration::in_profile(fs, self.new.clone())
        } else {
            Migration::user(fs, self.old.clone(), self.new.clone())
        }
    }

    pub(super) fn run<F: MigrationFs>(&self, fs: &F) -> Outcome {
        self.migration(fs).run(|| false)
    }

    pub(super) fn path(&self, rel: &str) -> PathBuf {
        self.temp.path().join(rel)
    }

    pub(super) fn write(&self, rel: &str, bytes: &[u8]) {
        let path = self.path(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn seed(&self) {
        let (config, data) = self.rel_roots();
        let db: Vec<u8> = (0..200_000u32).map(|n| (n * 7 % 251) as u8).collect();
        for (name, bytes) in [
            ("ferrite.toml", &b"theme = \"Ferrite Dark\"\n"[..]),
            ("ferrite.toml.defaults-pending", b"pending"),
            ("ferrite.toml.bak", b"backup"),
            ("keymap.toml", b"[[binding]]\n"),
            ("rename.log", b"# ferrite rename log format 1\n"),
            ("rename.log.1", b"# ferrite rename log format 1\nolder\n"),
        ] {
            self.write(&format!("{config}/{name}"), bytes);
        }
        for (name, bytes) in [
            ("ferrite.db", &db[..]),
            ("ferrite.db-wal", b"wal frames"),
            ("ferrite.db-shm", b"shm"),
            ("session.bin", b"FERRITE-SESSION\n"),
            ("cache.db", b"cache"),
            ("running", b""),
            ("ferrite.lock", b""),
            ("logs/ferrite.log", b"log"),
            ("index/walk-C.index", b"index"),
        ] {
            self.write(&format!("{data}/{name}"), bytes);
        }
    }

    pub(super) fn data_rel(&self) -> &'static str {
        self.rel_roots().1
    }

    pub(super) fn config_rel(&self) -> &'static str {
        self.rel_roots().0
    }

    pub(super) fn new_data_rel(&self) -> &'static str {
        if self.in_place() {
            "p/data"
        } else {
            "local/Navo"
        }
    }

    pub(super) fn new_config_rel(&self) -> &'static str {
        if self.in_place() {
            "p/config"
        } else {
            "roaming/Navo"
        }
    }

    pub(super) fn retired_data_rel(&self) -> &'static str {
        if self.in_place() {
            "p/data"
        } else {
            "local/Ferrite.migrated-to-navo"
        }
    }

    pub(super) fn journal_rel(&self) -> &'static str {
        if self.in_place() {
            "p/Navo-migration.json"
        } else {
            "local/Navo-migration.json"
        }
    }

    pub(super) fn read(&self, rel: &str) -> Option<Vec<u8>> {
        std::fs::read(self.path(rel)).ok()
    }

    pub(super) fn journal_phase(&self) -> Option<String> {
        let bytes = self.read(self.journal_rel())?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        value["phase"].as_str().map(str::to_owned)
    }

    pub(super) fn assert_started_fresh(&self, original: &Tree, aside: &[&str]) {
        let mut after = self.tree();
        let marker = format!("{}/{MARKER}", self.new_data_rel());
        assert!(
            after.remove(&marker).is_some(),
            "start fresh writes the marker"
        );
        if !self.in_place() {
            assert_eq!(after.remove("local/Navo"), Some(None));
        }
        for rel in aside {
            assert!(after.remove(*rel).is_some(), "{rel} is kept");
        }
        assert_eq!(Keys(&after), Keys(original), "file set after start fresh");
        assert!(after == *original, "old files differ after start fresh");
    }

    fn rel_roots(&self) -> (&'static str, &'static str) {
        if self.in_place() {
            ("p/config", "p/data")
        } else {
            ("roaming/Ferrite", "local/Ferrite")
        }
    }

    pub(super) fn tree(&self) -> Tree {
        let mut tree = Tree::new();
        walk(self.temp.path(), self.temp.path(), &mut tree);
        tree
    }

    pub(super) fn state(&self) -> Tree {
        let quoted = serde_json::to_string(self.temp.path()).unwrap();
        let temp = quoted.trim_matches('"').as_bytes();
        let mut tree = self.tree();
        for bytes in tree
            .values_mut()
            .flatten()
            .filter(|bytes| bytes.len() < 4096)
        {
            *bytes = replace_all(bytes, temp, b"<temp>");
        }
        tree
    }

    pub(super) fn expected_after(&self, original: &Tree) -> Tree {
        let mut tree = Tree::new();
        for (rel, bytes) in original {
            tree.insert(retired_name(rel, self.in_place()), bytes.clone());
        }
        for (rel, bytes) in original {
            if let Some(target) = migrated_name(rel, self.in_place()) {
                ensure_parents(&mut tree, &target);
                tree.insert(target, bytes.clone());
            }
        }
        let marker = if self.in_place() {
            format!("p/data/{MARKER}")
        } else {
            format!("local/Navo/{MARKER}")
        };
        ensure_parents(&mut tree, &marker);
        tree.insert(marker, Some(Vec::new()));
        tree
    }

    pub(super) fn assert_one_navo_profile(&self, original: &Tree) {
        let mut tree = self.tree();
        for (rel, bytes) in &mut tree {
            if rel.ends_with(MARKER) {
                *bytes = Some(Vec::new());
            }
        }
        assert_eq!(
            Keys(&tree),
            Keys(&self.expected_after(original)),
            "file set after migration"
        );
        assert!(
            tree == self.expected_after(original),
            "bytes after migration"
        );
    }
}

#[derive(PartialEq)]
struct Keys<'a>(&'a Tree);

impl std::fmt::Debug for Keys<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.0.keys()).finish()
    }
}

fn walk(base: &Path, dir: &Path, tree: &mut Tree) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let rel = path
            .strip_prefix(base)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if path.is_dir() {
            tree.insert(rel, None);
            walk(base, &path, tree);
        } else {
            tree.insert(rel, Some(std::fs::read(&path).unwrap()));
        }
    }
}

fn replace_all(bytes: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut rest = bytes;
    while !rest.is_empty() {
        if rest.starts_with(from) {
            out.extend_from_slice(to);
            rest = &rest[from.len()..];
        } else {
            out.push(rest[0]);
            rest = &rest[1..];
        }
    }
    out
}

fn ensure_parents(tree: &mut Tree, rel: &str) {
    let mut path = Path::new(rel).parent();
    while let Some(parent) = path.filter(|p| !p.as_os_str().is_empty()) {
        tree.insert(parent.to_string_lossy().into_owned(), None);
        path = parent.parent();
    }
}

const DETACHED: [&str; 3] = ["ferrite.db", "ferrite.db-wal", "session.bin"];

fn retired_name(rel: &str, in_place: bool) -> String {
    let (dir, name) = rel.rsplit_once('/').unwrap_or(("", rel));
    let in_data = dir == "local/Ferrite" || dir == "p/data";
    let name = if in_data && DETACHED.contains(&name) {
        format!("{name}.migrated-to-navo")
    } else {
        name.to_owned()
    };
    let dir = if in_place {
        dir.to_owned()
    } else {
        dir.replacen("/Ferrite", "/Ferrite.migrated-to-navo", 1)
    };
    let rel = if dir.is_empty() {
        name
    } else {
        format!("{dir}/{name}")
    };
    if !in_place && (rel == "roaming/Ferrite" || rel == "local/Ferrite") {
        return format!("{rel}.migrated-to-navo");
    }
    rel
}

fn migrated_name(rel: &str, in_place: bool) -> Option<String> {
    let (dir, name) = rel.rsplit_once('/')?;
    let new_name = match name {
        "ferrite.toml" => "navo.toml",
        "ferrite.toml.defaults-pending" => "navo.toml.defaults-pending",
        "keymap.toml" | "rename.log" | "rename.log.1" if !in_place => name,
        "ferrite.db" => "navo.db",
        "ferrite.db-wal" => "navo.db-wal",
        "session.bin" => name,
        _ => return None,
    };
    let dir = match dir {
        "roaming/Ferrite" => "roaming/Navo",
        "local/Ferrite" => "local/Navo",
        "p/config" | "p/data" => dir,
        _ => return None,
    };
    Some(format!("{dir}/{new_name}"))
}
