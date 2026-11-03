//! Repository badges kept in the profile's cache database, so a restart shows the last known
//! states at once. Every saved entry is shown as stale until a scan refreshes it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use navo_store::CacheDb;
use navo_ui::git::{BadgeKind, BadgeReason, RepoBadge};
use serde::{Deserialize, Serialize};

use super::cache::{Cached, State};

const VERSION: u32 = 1;
const MAX_AGE_SECS: i64 = 30 * 24 * 60 * 60;
const MAX_ENTRY_BYTES: usize = 256 * 1024;

/// Where scanned badges are kept between runs. Calls come from the scanner thread only.
pub(crate) trait SavedBadges: Send + Sync + 'static {
    /// Entries still worth showing, least recently saved first.
    fn load(&self) -> Vec<(PathBuf, Cached)>;

    fn save(&self, root: &Path, cached: &Cached);
}

enum Db {
    Closed(PathBuf),
    Open(Box<CacheDb>),
    Failed,
}

/// Opens the database on first use, so the cost never lands on the thread that builds it.
pub(crate) struct DbSaved {
    db: Mutex<Db>,
}

impl DbSaved {
    pub(crate) fn at(path: PathBuf) -> Self {
        Self {
            db: Mutex::new(Db::Closed(path)),
        }
    }

    fn with<T>(&self, run: impl FnOnce(&mut CacheDb) -> T) -> Option<T> {
        let mut db = self.db.lock().unwrap_or_else(PoisonError::into_inner);
        if let Db::Closed(path) = &*db {
            *db = match CacheDb::open(path) {
                Ok((opened, _)) => Db::Open(Box::new(opened)),
                Err(error) => {
                    tracing::debug!(%error, "the saved Git badges cannot be opened");
                    Db::Failed
                }
            };
        }
        match &mut *db {
            Db::Open(opened) => Some(run(opened)),
            _ => None,
        }
    }
}

impl SavedBadges for DbSaved {
    fn load(&self) -> Vec<(PathBuf, Cached)> {
        let now = now_secs();
        let kept = self.with(|db| {
            let rows = db
                .git_badges()
                .inspect_err(|error| tracing::debug!(%error, "saved Git badges cannot be read"))
                .unwrap_or_default();
            rows.into_iter()
                .filter_map(|(root, bytes)| usable(db, root, &bytes, now))
                .collect()
        });
        kept.unwrap_or_default()
    }

    fn save(&self, root: &Path, cached: &Cached) {
        let (Some(key), Some(bytes)) = (root.to_str(), encode(cached, now_secs())) else {
            return;
        };
        let saved = self.with(|db| db.put_git_badge(key, &bytes));
        if let Some(Err(error)) = saved {
            tracing::debug!(%error, "a Git badge cannot be saved");
        }
    }
}

/// Drops the entry from the database when it cannot be shown any more.
fn usable(db: &mut CacheDb, root: String, bytes: &[u8], now: i64) -> Option<(PathBuf, Cached)> {
    let cached = decode(bytes, now);
    if cached.is_none()
        && let Err(error) = db.remove_git_badge(&root)
    {
        tracing::debug!(%error, "an unusable saved Git badge stays in the database");
    }
    cached.map(|cached| (PathBuf::from(root), cached))
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

#[derive(Serialize, Deserialize)]
struct Header {
    version: u32,
    saved_at: i64,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    #[serde(flatten)]
    header: Header,
    root_name: String,
    badge: StoredBadge,
    dirs: BTreeMap<String, (String, Option<String>)>,
}

#[derive(Serialize, Deserialize)]
struct StoredBadge {
    kind: String,
    branch: Option<String>,
    upstream: Option<String>,
    ahead: u32,
    behind: u32,
    changed: u32,
    untracked: u32,
    conflicted: u32,
    fetched_ms: Option<i64>,
    reason: Option<String>,
}

pub(super) fn encode(cached: &Cached, saved_at: i64) -> Option<Vec<u8>> {
    let badge = &cached.root;
    let stored = Stored {
        header: Header {
            version: VERSION,
            saved_at,
        },
        root_name: cached.root_name.clone(),
        badge: StoredBadge {
            kind: kind_name(badge.kind).to_owned(),
            branch: badge.branch.clone(),
            upstream: badge.upstream.clone(),
            ahead: badge.ahead,
            behind: badge.behind,
            changed: badge.changed,
            untracked: badge.untracked,
            conflicted: badge.conflicted,
            fetched_ms: badge.fetched_ms,
            reason: badge.reason.map(|reason| reason_name(reason).to_owned()),
        },
        dirs: cached
            .dirs
            .iter()
            .map(|(dir, (kind, reason))| {
                let state = (
                    kind_name(*kind).to_owned(),
                    reason.map(|r| reason_name(r).to_owned()),
                );
                (dir.clone(), state)
            })
            .collect(),
    };
    let bytes = serde_json::to_vec(&stored).ok()?;
    (bytes.len() <= MAX_ENTRY_BYTES).then_some(bytes)
}

/// `None` for an unknown version, an entry older than 30 days, or bytes that do not parse.
pub(super) fn decode(bytes: &[u8], now: i64) -> Option<Cached> {
    let header: Header = serde_json::from_slice(bytes).ok()?;
    if header.version != VERSION || now.saturating_sub(header.saved_at) > MAX_AGE_SECS {
        return None;
    }
    let stored: Stored = serde_json::from_slice(bytes).ok()?;
    let badge = stored.badge;
    let root = RepoBadge {
        kind: kind_of(&badge.kind)?,
        is_root: true,
        stale: true,
        branch: badge.branch,
        upstream: badge.upstream,
        ahead: badge.ahead,
        behind: badge.behind,
        changed: badge.changed,
        untracked: badge.untracked,
        conflicted: badge.conflicted,
        fetched_ms: badge.fetched_ms,
        reason: optional_reason(badge.reason.as_deref())?,
    };
    let dirs = stored
        .dirs
        .into_iter()
        .map(|(dir, (kind, reason))| {
            let state: State = (kind_of(&kind)?, optional_reason(reason.as_deref())?);
            Some((dir, state))
        })
        .collect::<Option<_>>()?;
    Some(Cached {
        root,
        root_name: stored.root_name,
        dirs,
        stale: true,
    })
}

const KINDS: [(BadgeKind, &str); 8] = [
    (BadgeKind::Clean, "clean"),
    (BadgeKind::Modified, "modified"),
    (BadgeKind::Ahead, "ahead"),
    (BadgeKind::Behind, "behind"),
    (BadgeKind::Diverged, "diverged"),
    (BadgeKind::Conflict, "conflict"),
    (BadgeKind::Unknown, "unknown"),
    (BadgeKind::Checking, "checking"),
];

const REASONS: [(BadgeReason, &str); 4] = [
    (BadgeReason::TimedOut, "timed-out"),
    (BadgeReason::NoGit, "no-git"),
    (BadgeReason::NotTrusted, "not-trusted"),
    (BadgeReason::Failed, "failed"),
];

fn kind_name(kind: BadgeKind) -> &'static str {
    KINDS
        .iter()
        .find(|(k, _)| *k == kind)
        .map_or("unknown", |(_, name)| name)
}

fn kind_of(name: &str) -> Option<BadgeKind> {
    KINDS
        .iter()
        .find(|(_, n)| *n == name)
        .map(|(kind, _)| *kind)
}

fn reason_name(reason: BadgeReason) -> &'static str {
    REASONS
        .iter()
        .find(|(r, _)| *r == reason)
        .map_or("failed", |(_, name)| name)
}

/// `Some(None)` for no reason; `None` for a name this version does not know.
fn optional_reason(name: Option<&str>) -> Option<Option<BadgeReason>> {
    name.map_or(Some(None), |name| reason_of(name).map(Some))
}

fn reason_of(name: &str) -> Option<BadgeReason> {
    REASONS
        .iter()
        .find(|(_, n)| *n == name)
        .map(|(reason, _)| *reason)
}

#[cfg(test)]
#[path = "scan_tests/saved.rs"]
mod tests;
