use std::collections::{BTreeMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;

use navo_core::ProviderPath;
use navo_fs::DriveClass;
use navo_git::status::{Badge, Head, Reason, RepoStatus};
use navo_ui::git::{BadgeKind, BadgeReason, RepoBadge};

use super::cache::{Cached, State};
use super::queue::Listing;
use super::{Limits, Probe, REPO_CAP, ScanError};

pub(super) struct Binding {
    pub name: String,
    pub root: PathBuf,
    pub relative: String,
}

pub(super) struct Discovered {
    pub listing: Arc<Listing>,
    pub bindings: Vec<Binding>,
    pub roots: Vec<PathBuf>,
}

impl Discovered {
    fn empty(listing: Arc<Listing>) -> Self {
        Self {
            listing,
            bindings: Vec::new(),
            roots: Vec::new(),
        }
    }
}

/// Clears the flag when the helper thread ends, or never starts.
struct Helper(Arc<AtomicBool>);

impl Drop for Helper {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// `discover` for a network listing on a helper thread. One hung `has_git` costs the helper,
/// not the scanner: past `probe_timeout`, or while an earlier helper still hangs, the listing
/// gets no badges.
pub(super) fn discover_on_helper(
    listing: Arc<Listing>,
    probe: Arc<dyn Probe>,
    limits: Limits,
    running: Arc<AtomicBool>,
) -> Discovered {
    if running.swap(true, Ordering::SeqCst) {
        return Discovered::empty(listing);
    }
    let helper = Helper(running);
    let (sender, receiver) = mpsc::sync_channel(1);
    let own = listing.clone();
    let spawned = thread::Builder::new()
        .name("navo-git-probe".into())
        .spawn(move || {
            let found = discover(own, probe.as_ref(), true, limits);
            drop(helper);
            let _ = sender.send(found);
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "cannot start the Git probe helper");
        return Discovered::empty(listing);
    }
    receiver
        .recv_timeout(limits.probe_timeout)
        .unwrap_or_else(|_| Discovered::empty(listing))
}

pub(super) fn discover(
    listing: Arc<Listing>,
    probe: &dyn Probe,
    network: bool,
    limits: Limits,
) -> Discovered {
    let mut found = Discovered::empty(listing);
    if !network && probe.drive_class(&found.listing.folder) == DriveClass::Network {
        return found;
    }
    let start = probe.now();
    let parent = find_parent(
        &found.listing.path,
        probe,
        &found.listing,
        start + limits.probe_timeout,
    );
    let mut roots = HashSet::new();
    for row in &found.listing.rows {
        if found.listing.cancelled.is_cancelled()
            || probe.now().duration_since(start) >= limits.probe_timeout
        {
            break;
        }
        let Some(path) = row_path(&found.listing, &row.name) else {
            continue;
        };
        let own_repo = probe.has_git(&path);
        let root = if own_repo {
            Some(&path)
        } else {
            parent.as_ref()
        };
        let Some(root) = root else {
            continue;
        };
        if !roots.contains(root) && roots.len() >= REPO_CAP {
            continue;
        }
        if roots.insert(root.clone()) {
            found.roots.push(root.clone());
        }
        found.bindings.push(Binding {
            name: row.name.clone(),
            root: root.clone(),
            relative: relative(&path, root),
        });
    }
    found
}

fn find_parent(
    path: &Path,
    probe: &dyn Probe,
    listing: &Listing,
    deadline: std::time::Instant,
) -> Option<PathBuf> {
    for ancestor in path.ancestors() {
        if ancestor.as_os_str().is_empty()
            || listing.cancelled.is_cancelled()
            || probe.now() >= deadline
        {
            break;
        }
        if probe.has_git(ancestor) {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

fn row_path(listing: &Listing, name: &str) -> Option<PathBuf> {
    let location = listing.folder.join(&ProviderPath::from_text(name)).ok()?;
    navo_fs::os_path(&location).ok()
}

fn relative(path: &Path, root: &Path) -> String {
    let Ok(path) = path.strip_prefix(root) else {
        return String::new();
    };
    path.components()
        .filter_map(|part| match part {
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn project(
    result: Result<&RepoStatus, ScanError>,
    bindings: &[Binding],
    root: &Path,
) -> Cached {
    let mut dirs = BTreeMap::new();
    for binding in bindings
        .iter()
        .filter(|b| b.root == root && !b.relative.is_empty())
    {
        let state = match result {
            Ok(status) => state(status.dir_badge(&binding.relative)),
            Err(error) => (BadgeKind::Unknown, Some(reason(error))),
        };
        dirs.insert(binding.relative.clone(), state);
    }
    Cached {
        root: root_badge(result),
        dirs,
        stale: false,
    }
}

fn state(badge: Badge) -> State {
    let kind = match badge {
        Badge::Conflict => BadgeKind::Conflict,
        Badge::Modified => BadgeKind::Modified,
        Badge::Diverged => BadgeKind::Diverged,
        Badge::Ahead => BadgeKind::Ahead,
        Badge::Behind => BadgeKind::Behind,
        Badge::Clean => BadgeKind::Clean,
        Badge::Unknown(_) => BadgeKind::Unknown,
    };
    let reason = match badge {
        Badge::Unknown(Reason::TimedOut) => Some(BadgeReason::TimedOut),
        Badge::Unknown(Reason::NoGit) => Some(BadgeReason::NoGit),
        Badge::Unknown(Reason::NotTrusted) => Some(BadgeReason::NotTrusted),
        Badge::Unknown(Reason::Failed) => Some(BadgeReason::Failed),
        _ => None,
    };
    (kind, reason)
}

fn reason(error: ScanError) -> BadgeReason {
    match error {
        ScanError::TimedOut => BadgeReason::TimedOut,
        ScanError::NoGit => BadgeReason::NoGit,
        ScanError::NotTrusted => BadgeReason::NotTrusted,
        ScanError::Failed | ScanError::Cancelled => BadgeReason::Failed,
    }
}

fn root_badge(result: Result<&RepoStatus, ScanError>) -> RepoBadge {
    let mut badge = RepoBadge {
        kind: BadgeKind::Unknown,
        stale: false,
        is_root: true,
        branch: None,
        upstream: None,
        ahead: 0,
        behind: 0,
        changed: 0,
        untracked: 0,
        conflicted: 0,
        fetched_ms: None,
        reason: None,
    };
    let status = match result {
        Ok(status) => status,
        Err(error) => {
            badge.reason = Some(reason(error));
            return badge;
        }
    };
    (badge.kind, badge.reason) = state(status.dir_badge(""));
    badge.branch = match &status.head {
        Head::Branch(name) | Head::Unborn(name) => Some(name.clone()),
        Head::Detached(_) => None,
    };
    if let Some(upstream) = &status.upstream {
        badge.upstream = Some(upstream.name.clone());
        badge.ahead = upstream.ahead;
        badge.behind = upstream.behind;
    }
    badge.changed = status.changed;
    badge.untracked = status.untracked;
    badge.conflicted = status.conflicted;
    badge.fetched_ms = status.fetched_ms;
    badge
}
