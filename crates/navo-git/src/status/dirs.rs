use std::collections::HashMap;

use super::Badge;

const MAX_DIRS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Severity {
    Modified,
    Conflict,
}

/// Worst state below each folder of one repository, built from one status.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirMap {
    worst: HashMap<String, Severity>,
    overflow: bool,
}

impl DirMap {
    /// Marks every folder above `path` (repository-relative, `/` separated).
    /// A path ending in `/` marks that folder too.
    pub(crate) fn note(&mut self, path: &[u8], severity: Severity) {
        let path = String::from_utf8_lossy(path);
        for (index, _) in path.match_indices('/') {
            if index == 0 {
                continue;
            }
            self.raise(&path[..index], severity);
        }
    }

    fn raise(&mut self, dir: &str, severity: Severity) {
        if let Some(worst) = self.worst.get_mut(dir) {
            *worst = (*worst).max(severity);
        } else if self.worst.len() < MAX_DIRS {
            self.worst.insert(dir.to_owned(), severity);
        } else {
            self.overflow = true;
        }
    }

    /// Number of folders with a recorded state.
    pub fn len(&self) -> usize {
        self.worst.len()
    }

    /// True when no folder has a recorded state.
    pub fn is_empty(&self) -> bool {
        self.worst.is_empty()
    }

    pub(super) fn overflowed(&self) -> bool {
        self.overflow
    }

    pub(super) fn badge(&self, dir: &str) -> Badge {
        match self.worst.get(dir) {
            Some(Severity::Conflict) => Badge::Conflict,
            Some(Severity::Modified) => Badge::Modified,
            None => Badge::Clean,
        }
    }
}
