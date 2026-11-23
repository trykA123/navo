//! Profile scope for index hits: other users' profile folders stay out of search results and
//! folder totals (D126). The service reads the profiles root, the installing user's profile and
//! every other profile once at start; each query maps them to table ids, so a hit costs integer
//! compares only.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use navo_index::table::{FolderLookup, NameTable};

use super::service_log::Log;

/// Registry key that lists the profiles root and one subkey per account SID.
pub(super) const PROFILE_LIST: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList";
/// The shared profile under the profiles root that every user may see.
const PUBLIC: &str = "Public";
/// Upper bound for mapping the scope's folder paths to table ids while the read lock is held.
pub(super) const PROFILE_LOOKUP_BUDGET: Duration = Duration::from_secs(1);
/// Deepest folder chain walked when deciding whether a folder lies under a hidden one.
const MAX_DEPTH: usize = 4096;

/// Folder paths, as `X:\folder`, that decide what one user may see. A folder may be listed
/// twice: as the registry names it and as its final path behind a junction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ProfileScope {
    /// Folders whose children are hidden unless kept: the profiles root and the own profile's
    /// ancestors below it.
    guarded: Vec<String>,
    /// Children of guarded folders that stay visible: the own profile, `Public`, the ancestors.
    kept: Vec<String>,
    /// Other users' profiles, hidden wherever they are.
    hidden: Vec<String>,
    /// Why the list of other profiles could not be read; only the root's children are hidden.
    others_unread: Option<String>,
}

/// Why the profile scope could not be read; nothing is hidden then.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum ProfileUnresolved {
    #[error("the profiles root is missing from the registry")]
    MissingRoot,
    #[error("the account has no profile path in the registry")]
    MissingProfile,
    #[error("profile path `{0}` is not a folder on a lettered drive")]
    NotLocal(String),
}

/// Where the scope comes from: the registry and the file system on Windows, maps in tests.
pub(super) trait ProfileSource {
    /// Reads the string `value` under `HKLM\key`, environment strings expanded.
    fn text(&self, key: &str, value: &str) -> Option<String>;
    /// Lists the subkey names of `HKLM\key`; fails with a reason when the key cannot be read.
    fn subkeys(&self, key: &str) -> Result<Vec<String>, String>;
    /// Resolves `path` through junctions and links; `None` when it cannot be opened.
    fn final_path(&self, path: &str) -> Option<String>;
}

impl ProfileScope {
    /// Builds a scope from two paths such as `C:\Users` and `C:\Users\me`; fails when either
    /// is not a folder below a drive letter (a UNC path, `\\?\` form or a bare drive).
    /// Only tests build a scope from fixed paths; the service reads it from the registry.
    #[cfg(test)]
    pub fn new(root: &str, user: &str) -> Result<Self, ProfileUnresolved> {
        Ok(Self::build(
            &[local_folder(root)?],
            &[local_folder(user)?],
            Vec::new(),
        ))
    }

    /// Derives the guarded, kept and hidden folders from the root and own-profile forms and the
    /// other profiles; an other profile equal to the own one is dropped, and one that contains
    /// it is guarded like the root, never hidden.
    fn build(roots: &[String], users: &[String], others: Vec<String>) -> Self {
        let mut guarded = roots.to_vec();
        let mut kept = users.to_vec();
        kept.extend(roots.iter().map(|root| format!("{root}\\{PUBLIC}")));
        let (containing, hidden): (Vec<String>, Vec<String>) = others
            .into_iter()
            .partition(|other| users.iter().any(|user| at_or_below(user, other)));
        guarded.extend(
            containing
                .into_iter()
                .filter(|other| !users.iter().any(|user| fold(user) == fold(other))),
        );
        for root in guarded.clone() {
            for user in users {
                let between = ancestors_below(&root, user);
                guarded.extend(between.iter().cloned());
                kept.extend(between);
            }
        }
        Self {
            guarded,
            kept,
            hidden,
            others_unread: None,
        }
    }

    /// Why other profiles outside the root stay searchable, when the registry list failed.
    pub fn others_unread(&self) -> Option<&str> {
        self.others_unread.as_deref()
    }
}

/// Reads the scope for `sid` from `source`: the root and own profile in both their registry
/// and final forms, and every other account's profile; fails when the root or own is unusable.
pub(super) fn scope_from(
    sid: &str,
    source: &impl ProfileSource,
) -> Result<ProfileScope, ProfileUnresolved> {
    let root = source
        .text(PROFILE_LIST, "ProfilesDirectory")
        .ok_or(ProfileUnresolved::MissingRoot)?;
    let user = source
        .text(&format!(r"{PROFILE_LIST}\{sid}"), "ProfileImagePath")
        .ok_or(ProfileUnresolved::MissingProfile)?;
    let roots = both_forms(source, &root)?;
    let users = both_forms(source, &user)?;
    let (others, others_unread) = match source.subkeys(PROFILE_LIST) {
        Ok(sids) => (other_profiles(sid, &sids, source), None),
        Err(reason) => (Vec::new(), Some(reason)),
    };
    Ok(ProfileScope {
        others_unread,
        ..ProfileScope::build(&roots, &users, others)
    })
}

/// Collects both forms of every profile in `sids` except `own`; entries without a usable local
/// path (no path, a network home) are skipped because no local index holds them.
fn other_profiles(own: &str, sids: &[String], source: &impl ProfileSource) -> Vec<String> {
    let mut others = Vec::new();
    for sid in sids.iter().filter(|sid| !sid.eq_ignore_ascii_case(own)) {
        let path = source.text(&format!(r"{PROFILE_LIST}\{sid}"), "ProfileImagePath");
        // A REG_SZ value with `%…%` stays unexpanded, fails `local_folder` and is skipped.
        if let Some(paths) = path.and_then(|path| both_forms(source, &path).ok()) {
            others.extend(paths);
        }
    }
    others
}

/// Returns `path` and, when it differs, its final path without the `\\?\` prefix; fails when
/// `path` itself is not local. A final path that is not local is dropped.
fn both_forms(source: &impl ProfileSource, path: &str) -> Result<Vec<String>, ProfileUnresolved> {
    let named = local_folder(path)?;
    let resolved = source.final_path(path).and_then(|resolved| {
        let plain = resolved.strip_prefix(r"\\?\").unwrap_or(&resolved);
        local_folder(plain).ok()
    });
    let differs = resolved.filter(|resolved| fold(resolved) != fold(&named));
    Ok(std::iter::once(named).chain(differs).collect())
}

/// Reads the installing user's scope from the registry `ProfileList` and the file system.
#[cfg(windows)]
pub(super) fn installing_user(sid: &str) -> Result<ProfileScope, ProfileUnresolved> {
    scope_from(sid, &super::profiles_windows::System)
}

/// Trims a `X:\a\b` path to `X:\a\b` with backslashes; rejects anything not below a drive letter.
fn local_folder(path: &str) -> Result<String, ProfileUnresolved> {
    let trimmed = path.trim().trim_end_matches(['\\', '/']);
    let bytes = trimmed.as_bytes();
    let lettered = bytes.len() > 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    if lettered
        && !trimmed[3..]
            .split(['\\', '/'])
            .any(|part| part.is_empty() || part == "..")
    {
        Ok(trimmed.replace('/', "\\"))
    } else {
        Err(ProfileUnresolved::NotLocal(path.to_owned()))
    }
}

/// Folds case the way the table's folder lookup does.
fn fold(path: &str) -> String {
    path.to_lowercase()
}

/// True when `path` is `folder` or lies below it, comparing folder names without case.
fn at_or_below(path: &str, folder: &str) -> bool {
    let (path, folder): (Vec<&str>, Vec<&str>) =
        (path.split('\\').collect(), folder.split('\\').collect());
    path.len() >= folder.len() && folder.iter().zip(&path).all(|(a, b)| fold(a) == fold(b))
}

/// The folders strictly between `root` and `user`, as full paths; empty when `user` is a
/// direct child of `root` or not below it.
fn ancestors_below(root: &str, user: &str) -> Vec<String> {
    let depth = root.split('\\').count();
    let parts: Vec<&str> = user.split('\\').collect();
    if !at_or_below(user, root) {
        return Vec::new();
    }
    (depth + 1..parts.len())
        .map(|end| parts[..end].join("\\"))
        .collect()
}

/// Table ids that decide which entries one query may return.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ProfileIds {
    /// Nothing is hidden: no scope, or none of its folders is on this volume.
    Open,
    /// Children of `guarded` are hidden unless `kept`; `hidden` are hidden wherever they are.
    Guarded {
        guarded: Vec<u32>,
        kept: Vec<u32>,
        hidden: Vec<u32>,
    },
    /// The folder lookup ran out of time: every entry is hidden (fail closed).
    Closed,
}

impl ProfileIds {
    /// Maps the scope's folders through `lookup`; `Closed` when `deadline` passes first.
    pub fn resolve_with(
        scope: Option<&ProfileScope>,
        lookup: &mut FolderLookup<'_>,
        deadline: Instant,
    ) -> Self {
        let Some(scope) = scope else {
            return Self::Open;
        };
        let mut ids = |paths: &[String]| -> Option<Vec<u32>> {
            let mut found = Vec::new();
            for path in paths {
                found.extend(lookup.find_folder(path, deadline).ok()?);
            }
            Some(found)
        };
        let (Some(guarded), Some(kept), Some(hidden)) =
            (ids(&scope.guarded), ids(&scope.kept), ids(&scope.hidden))
        else {
            return Self::Closed;
        };
        if guarded.is_empty() && hidden.is_empty() {
            return Self::Open;
        }
        Self::Guarded {
            guarded,
            kept,
            hidden,
        }
    }

    /// True when the entry `id`, whose parent is `parent`, is hidden with everything below it.
    pub fn hides(&self, id: u32, parent: Option<u32>) -> bool {
        match self {
            Self::Open => false,
            Self::Closed => true,
            Self::Guarded {
                guarded,
                kept,
                hidden,
            } => {
                hidden.contains(&id)
                    || (parent.is_some_and(|parent| guarded.contains(&parent))
                        && !kept.contains(&id))
            }
        }
    }

    /// True when `id` or any folder above it is hidden; a chain deeper than the walk limit
    /// counts as hidden (fail closed).
    pub fn hides_path(&self, table: &NameTable, id: u32) -> bool {
        let mut current = id;
        for _ in 0..MAX_DEPTH {
            let parent = table.parent(current);
            if self.hides(current, parent) {
                return true;
            }
            match parent {
                Some(parent) => current = parent,
                None => return false,
            }
        }
        true
    }
}

/// Writes one service log line, once per service start, when a lookup timeout hides hits.
pub(super) struct TimeoutNotice {
    log: Log,
    said: AtomicBool,
}

impl TimeoutNotice {
    /// Wraps the service log; nothing is written until the first timeout.
    pub fn new(log: Log) -> Self {
        Self {
            log,
            said: AtomicBool::new(false),
        }
    }

    /// Records that a profile lookup timed out and hid a volume's hits; only the first call
    /// writes the log line.
    pub fn timed_out(&self) {
        if !self.said.swap(true, Ordering::Relaxed) {
            self.log.line(format_args!(
                "profile lookup timed out; a volume's hits were hidden for that query"
            ));
        }
    }
}

#[cfg(test)]
#[path = "profiles_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "profiles_scope_tests.rs"]
mod scope_tests;
