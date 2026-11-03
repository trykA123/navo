use std::collections::BTreeSet;

use gix::bstr::BString;
use gix::status::Item;
use gix::status::index_worktree::iter::Summary;

use crate::status::{DirMap, Severity};

const TRACKED_RECORD: usize = 114;
const UNMERGED_RECORD: usize = 162;
const UNTRACKED_RECORD: usize = 3;

/// Paths of one status run, deduplicated, with the size `git status --porcelain=v2` would print.
#[derive(Default)]
pub(super) struct Tally {
    changed: BTreeSet<BString>,
    conflicted: BTreeSet<BString>,
    untracked: BTreeSet<BString>,
    bytes: usize,
}

pub(super) struct Counts {
    pub changed: u32,
    pub untracked: u32,
    pub conflicted: u32,
    pub dirs: DirMap,
}

impl Tally {
    /// Adds one item; false once the estimated output passes `cap`.
    pub(super) fn add(&mut self, item: &Item, cap: usize) -> bool {
        match item {
            Item::TreeIndex(change) => self.insert(Kind::Changed, change.location().to_owned()),
            Item::IndexWorktree(item) => {
                let path = item.rela_path().to_owned();
                match item.summary() {
                    Some(Summary::Conflict) => self.insert(Kind::Conflicted, path),
                    Some(Summary::Added) => self.insert(Kind::Untracked, folder_path(item, path)),
                    Some(_) => self.insert(Kind::Changed, path),
                    None => {}
                }
            }
        }
        self.bytes <= cap
    }

    pub(super) fn counts(mut self) -> Counts {
        for path in &self.conflicted {
            self.changed.remove(path);
        }
        let mut dirs = DirMap::default();
        for path in &self.conflicted {
            dirs.note(path, Severity::Conflict);
        }
        for path in self.changed.iter().chain(&self.untracked) {
            dirs.note(path, Severity::Modified);
        }
        Counts {
            changed: count(&self.changed),
            untracked: count(&self.untracked),
            conflicted: count(&self.conflicted),
            dirs,
        }
    }

    fn insert(&mut self, kind: Kind, path: BString) {
        let (set, fixed) = match kind {
            Kind::Changed => (&mut self.changed, TRACKED_RECORD),
            Kind::Conflicted => (&mut self.conflicted, UNMERGED_RECORD),
            Kind::Untracked => (&mut self.untracked, UNTRACKED_RECORD),
        };
        let size = fixed + path.len();
        if set.insert(path) {
            self.bytes = self.bytes.saturating_add(size);
        }
    }
}

enum Kind {
    Changed,
    Conflicted,
    Untracked,
}

fn count(set: &BTreeSet<BString>) -> u32 {
    u32::try_from(set.len()).unwrap_or(u32::MAX)
}

/// A collapsed untracked folder ends in `/`, as in `git status`.
fn folder_path(item: &gix::status::index_worktree::Item, mut path: BString) -> BString {
    let is_folder = matches!(
        item,
        gix::status::index_worktree::Item::DirectoryContents { entry, .. }
            if entry.disk_kind == Some(gix::dir::entry::Kind::Directory)
    );
    if is_folder {
        path.push(b'/');
    }
    path
}
