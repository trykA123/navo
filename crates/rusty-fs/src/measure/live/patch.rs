use rusty_core::ProviderPath;

use super::known::{Known, KnownPaths};
use crate::measure::kinds::{Tally, add_kind, extension_of, sort_kinds, take_kind};
use crate::measure::largest::LargeFile;
use crate::measure::profile::FolderProfile;
use crate::{FsEntry, WatchChange};

pub(super) struct Subtree {
    pub files: Vec<(String, FsEntry)>,
    pub folders: Vec<String>,
    pub is_complete: bool,
}

pub(super) enum Look {
    File(FsEntry),
    Folder(Option<Subtree>),
    Missing,
    Unreadable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Verdict {
    Patched,
    Recount,
}

pub(super) struct Patch {
    profile: FolderProfile,
    known: KnownPaths,
    separator: char,
}

pub(super) fn relative_of(name: &ProviderPath, separator: char) -> String {
    let text = name.to_text();
    let text = if separator == '\\' {
        text.replace('/', "\\")
    } else {
        text
    };
    text.trim_matches(separator).to_owned()
}

fn leaf(path: &str, separator: char) -> &str {
    path.rsplit(separator).next().unwrap_or(path)
}

fn one(bytes: u64) -> Tally {
    Tally { bytes, files: 1 }
}

impl Patch {
    pub(super) fn new(profile: FolderProfile, separator: char) -> Self {
        Self {
            profile,
            known: KnownPaths::default(),
            separator,
        }
    }

    pub(super) fn view(&self) -> FolderProfile {
        let mut profile = self.profile.clone();
        profile.largest = profile.ranking.top();
        profile
    }

    pub(super) fn room(&self) -> usize {
        self.known.room()
    }

    pub(super) fn apply(&mut self, change: &WatchChange, look: Look) -> Verdict {
        let separator = self.separator;
        let verdict = match change {
            WatchChange::Created(name) => self.created(&relative_of(name, separator), look),
            WatchChange::Modified(name) => self.modified(&relative_of(name, separator), look),
            WatchChange::Removed(name) => self.removed(&relative_of(name, separator)),
            WatchChange::Renamed { from, to } => {
                let (from, to) = (relative_of(from, separator), relative_of(to, separator));
                self.renamed(&from, &to, look)
            }
        };
        sort_kinds(&mut self.profile.kinds);
        if self.profile.ranking.is_short() {
            return Verdict::Recount;
        }
        verdict
    }

    fn state_of(&self, path: &str) -> Option<Known> {
        self.known
            .get(path)
            .or_else(|| self.profile.ranking.size_of(path).map(Known::File))
    }

    fn created(&mut self, path: &str, look: Look) -> Verdict {
        match (look, self.state_of(path)) {
            (Look::File(entry), Some(Known::File(old))) => self.resize(path, old, &entry),
            (Look::File(entry), Some(Known::Gone) | None) => self.add_file(path, &entry),
            (Look::Folder(subtree), None | Some(Known::Gone)) => {
                return self.add_folder(path, subtree);
            }
            (Look::File(_), Some(Known::Folder)) | (Look::Folder(_), Some(Known::File(_))) => {
                return Verdict::Recount;
            }
            (Look::Missing, None) => self.known.insert(path.to_owned(), Known::Gone),
            _ => {}
        }
        Verdict::Patched
    }

    fn modified(&mut self, path: &str, look: Look) -> Verdict {
        let Look::File(entry) = look else {
            return Verdict::Patched;
        };
        match self.state_of(path) {
            Some(Known::File(old)) => self.resize(path, old, &entry),
            Some(Known::Gone) => self.add_file(path, &entry),
            Some(Known::Folder) => return Verdict::Recount,
            None => self.learn(path, &entry),
        }
        Verdict::Patched
    }

    fn removed(&mut self, path: &str) -> Verdict {
        match self.state_of(path) {
            Some(Known::File(bytes)) => self.drop_file(path, bytes),
            Some(Known::Gone) => {
                self.known.remove(path);
            }
            Some(Known::Folder) => self.drop_folder(path),
            None => return Verdict::Recount,
        }
        Verdict::Patched
    }

    fn renamed(&mut self, from: &str, to: &str, look: Look) -> Verdict {
        let Look::File(entry) = look else {
            self.known.rename(from, to, self.separator);
            self.profile.ranking.rename(from, to, self.separator);
            return Verdict::Patched;
        };
        match self.state_of(from) {
            Some(Known::File(old)) => {
                self.drop_file(from, old);
                self.add_file(to, &entry);
            }
            Some(Known::Gone) => {
                self.known.remove(from);
                self.add_file(to, &entry);
            }
            Some(Known::Folder) => return Verdict::Recount,
            None => self.move_counted(from, to, &entry),
        }
        Verdict::Patched
    }

    fn add_file(&mut self, path: &str, entry: &FsEntry) {
        let totals = &mut self.profile.totals;
        totals.files += 1;
        totals.bytes = totals.bytes.saturating_add(entry.size);
        let name = leaf(path, self.separator);
        add_kind(
            &mut self.profile.kinds,
            &mut self.profile.other,
            name,
            one(entry.size),
        );
        self.learn(path, entry);
    }

    fn drop_file(&mut self, path: &str, bytes: u64) {
        let totals = &mut self.profile.totals;
        totals.files = totals.files.saturating_sub(1);
        totals.bytes = totals.bytes.saturating_sub(bytes);
        let name = leaf(path, self.separator);
        take_kind(
            &mut self.profile.kinds,
            &mut self.profile.other,
            name,
            one(bytes),
        );
        self.profile.ranking.remove(path);
        self.known.remove(path);
    }

    fn resize(&mut self, path: &str, old: u64, entry: &FsEntry) {
        let totals = &mut self.profile.totals;
        totals.bytes = totals.bytes.saturating_sub(old).saturating_add(entry.size);
        let name = leaf(path, self.separator);
        let (kinds, other) = (&mut self.profile.kinds, &mut self.profile.other);
        take_kind(
            kinds,
            other,
            name,
            Tally {
                bytes: old,
                files: 0,
            },
        );
        add_kind(
            kinds,
            other,
            name,
            Tally {
                bytes: entry.size,
                files: 0,
            },
        );
        self.learn(path, entry);
    }

    fn learn(&mut self, path: &str, entry: &FsEntry) {
        self.profile.newest_ms = self.profile.newest_ms.max(entry.modified_ms);
        self.profile.ranking.offer(LargeFile {
            path: path.to_owned(),
            bytes: entry.size,
        });
        self.known.insert(path.to_owned(), Known::File(entry.size));
    }

    fn move_counted(&mut self, from: &str, to: &str, entry: &FsEntry) {
        let (old_name, new_name) = (leaf(from, self.separator), leaf(to, self.separator));
        if extension_of(old_name) != extension_of(new_name) {
            let (kinds, other) = (&mut self.profile.kinds, &mut self.profile.other);
            take_kind(kinds, other, old_name, one(entry.size));
            add_kind(kinds, other, new_name, one(entry.size));
        }
        self.learn(to, entry);
    }

    fn add_folder(&mut self, path: &str, subtree: Option<Subtree>) -> Verdict {
        let subtree = subtree.unwrap_or(Subtree {
            files: Vec::new(),
            folders: Vec::new(),
            is_complete: true,
        });
        if !subtree.is_complete {
            return Verdict::Recount;
        }
        for folder in std::iter::once(path.to_owned()).chain(subtree.folders) {
            if self.known.get(&folder) != Some(Known::Folder) {
                self.profile.totals.folders += 1;
                self.known.insert(folder, Known::Folder);
            }
        }
        for (file, entry) in &subtree.files {
            match self.state_of(file) {
                Some(Known::File(old)) => self.resize(file, old, entry),
                _ => self.add_file(file, entry),
            }
        }
        Verdict::Patched
    }

    fn drop_folder(&mut self, path: &str) {
        for (below, known) in self.known.take_below(path, self.separator) {
            match known {
                Known::File(bytes) => self.drop_file(&below, bytes),
                Known::Folder => {
                    self.profile.totals.folders = self.profile.totals.folders.saturating_sub(1);
                }
                Known::Gone => {}
            }
        }
        self.known.remove(path);
        self.profile.totals.folders = self.profile.totals.folders.saturating_sub(1);
    }
}
