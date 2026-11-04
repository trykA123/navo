use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::rc::Rc;

use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use navo_core::Location;

use crate::{EntryKind, FsEntry, os_path};

const GITIGNORE: &str = ".gitignore";
/// The compare rules cap; a longer file is read up to here.
const RULES_LIMIT: u64 = 1024 * 1024;

/// The `.gitignore` files that apply to one folder, nearest folder first.
#[derive(Clone, Debug, Default)]
pub(super) struct IgnoreRules {
    top: Option<Rc<Layer>>,
}

#[derive(Debug)]
struct Layer {
    rules: Gitignore,
    parent: Option<Rc<Layer>>,
}

impl IgnoreRules {
    /// Rules from the `.gitignore` in every folder above `root` up to its repository's top folder;
    /// none outside a repository or when `root` is that top folder, as in git.
    pub(super) fn above(root: &Location) -> Self {
        if os_path(root).is_ok_and(|path| is_repository(&path)) {
            return Self::default();
        }
        let mut folders = Vec::new();
        for folder in std::iter::successors(root.parent(), Location::parent) {
            let Ok(path) = os_path(&folder) else { continue };
            let top = is_repository(&path);
            folders.push(path);
            if top {
                return folders
                    .iter()
                    .rev()
                    .fold(Self::default(), |rules, path| rules.with_folder(path));
            }
        }
        Self::default()
    }

    /// These rules plus `folder/.gitignore`, when it exists and holds a rule.
    pub(super) fn with_folder(&self, folder: &Path) -> Self {
        match load(folder) {
            Some(rules) => Self {
                top: Some(Rc::new(Layer {
                    rules,
                    parent: self.top.clone(),
                })),
            },
            None => self.clone(),
        }
    }

    /// Git precedence: the nearest `.gitignore` with a matching line decides.
    pub(super) fn ignores(&self, location: &Location, is_folder: bool) -> bool {
        let Some(top) = self.top.as_deref() else {
            return false;
        };
        let Ok(path) = os_path(location) else {
            return false;
        };
        let mut layer = Some(top);
        while let Some(current) = layer {
            match current.rules.matched(&path, is_folder) {
                Match::Ignore(_) => return true,
                Match::Whitelist(_) => return false,
                Match::None => layer = current.parent.as_deref(),
            }
        }
        false
    }
}

fn is_repository(folder: &Path) -> bool {
    folder.join(".git").symlink_metadata().is_ok()
}

pub(super) fn is_gitignore_file(entry: &FsEntry) -> bool {
    entry.kind == EntryKind::File && entry.name.as_str() == Some(GITIGNORE)
}

fn load(folder: &Path) -> Option<Gitignore> {
    let read = || -> io::Result<Option<Vec<u8>>> {
        let path = folder.join(GITIGNORE);
        if !std::fs::symlink_metadata(&path)?.file_type().is_file() {
            return Ok(None);
        }
        let file = File::open(path)?;
        if !file.metadata()?.is_file() {
            return Ok(None);
        }
        let mut bytes = Vec::new();
        file.take(RULES_LIMIT).read_to_end(&mut bytes)?;
        Ok(Some(bytes))
    };
    let bytes = read()
        .map_err(|error| {
            if error.kind() != io::ErrorKind::NotFound {
                tracing::debug!(kind = ?error.kind(), "gitignore read failed");
            }
        })
        .ok()??;
    let lossy = String::from_utf8_lossy(&bytes);
    let text = lossy.strip_prefix('\u{feff}').unwrap_or(&lossy);
    let mut builder = GitignoreBuilder::new(folder);
    builder.case_insensitive(cfg!(windows)).ok()?;
    for line in text.lines() {
        if let Err(error) = builder.add_line(None, line) {
            tracing::debug!(%error, "gitignore line skipped");
        }
    }
    builder.build().ok().filter(|rules| !rules.is_empty())
}
