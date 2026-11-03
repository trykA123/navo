use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use navo_core::Location;
use navo_git::{Cancel, SyncKind};
use navo_store::GitSettings;
use navo_ui::git::{GitAction, GitMenu, Previewed, Shown, ToolRow};

use super::hooks::lock;
use super::launch::start::{self, Ports};
use super::launch::{Launch, LaunchError};
use super::menu;
use super::pullpush::{CliSync, Confirmation, SyncFailure, SyncPort};
use super::texts;
use super::tools::env::SystemEnv;
use super::tools::{self, DetectedTool, ToolKind};

const PENDING_CAP: usize = 8;

/// Finding and starting Git tools on this machine.
pub(super) trait ToolPort: Send + Sync {
    fn detect(&self) -> Vec<DetectedTool>;
    fn start(
        &self,
        launch: Launch,
        ports: &Ports,
        on_exit: Box<dyn FnOnce() + Send>,
    ) -> Result<(), LaunchError>;
}

struct SystemTools;

impl ToolPort for SystemTools {
    fn detect(&self) -> Vec<DetectedTool> {
        tools::detect(&SystemEnv)
    }

    fn start(
        &self,
        launch: Launch,
        ports: &Ports,
        on_exit: Box<dyn FnOnce() + Send>,
    ) -> Result<(), LaunchError> {
        start::start(launch, ports, on_exit)
    }
}

/// What the menu showed for one Pull or Push; the next run of it uses exactly this.
struct Pending {
    repo: Location,
    kind: SyncKind,
    /// The confirmation text the menu received; a run must name it.
    shown: Option<String>,
    /// Never reused, so it names this preview's target alone; a run must name it.
    id: String,
    outcome: Result<Confirmation, SyncFailure>,
}

/// The user's Git tool: detection, menu items, and the confirmations the menu showed.
pub(super) struct Tools {
    pub(super) port: Arc<dyn ToolPort>,
    pub(super) sync: Arc<dyn SyncPort>,
    pub(super) ports: Mutex<Ports>,
    detected: Mutex<Option<Vec<DetectedTool>>>,
    pending: Mutex<Vec<Pending>>,
    previewed: AtomicU64,
    runs: Mutex<Vec<(Location, Cancel)>>,
}

/// The claim on a repository while its Pull or Push runs; dropping it frees the repository.
pub(super) struct Run<'a> {
    tools: &'a Tools,
    repo: Location,
    pub(super) cancel: Cancel,
}

impl Drop for Run<'_> {
    fn drop(&mut self) {
        lock(&self.tools.runs).retain(|(held, _)| *held != self.repo);
    }
}

impl Tools {
    pub(super) fn new(port: Arc<dyn ToolPort>, sync: Arc<dyn SyncPort>) -> Self {
        let unset = Ports {
            terminal: Arc::new(|_| Err(navo_l10n::tr("terminal-not-configured"))),
            url: Arc::new(|_| {}),
        };
        Self {
            port,
            sync,
            ports: Mutex::new(unset),
            detected: Mutex::new(None),
            pending: Mutex::new(Vec::new()),
            previewed: AtomicU64::new(0),
            runs: Mutex::new(Vec::new()),
        }
    }

    /// Claims `repo` for one Pull or Push; `None` while another one runs there.
    pub(super) fn begin_run(&self, repo: &Location) -> Option<Run<'_>> {
        let mut runs = lock(&self.runs);
        if runs.iter().any(|(held, _)| held == repo) {
            return None;
        }
        let cancel = Cancel::default();
        runs.push((repo.clone(), cancel.clone()));
        Some(Run {
            tools: self,
            repo: repo.clone(),
            cancel,
        })
    }

    /// Cancels every running Pull and Push; each run then ends with its cancelled result.
    pub(super) fn cancel_runs(&self) {
        for (_, cancel) in lock(&self.runs).iter() {
            cancel.cancel();
        }
    }

    pub(super) fn system() -> Self {
        Self::new(Arc::new(SystemTools), Arc::new(CliSync))
    }

    /// Detection runs once per session; later calls reuse the result.
    pub(super) fn detected(&self) -> Vec<DetectedTool> {
        if let Some(found) = lock(&self.detected).clone() {
            return found;
        }
        let found = self.port.detect();
        *lock(&self.detected) = Some(found.clone());
        found
    }

    /// Detects again and replaces the result in one step, so readers never see an empty list.
    pub(super) fn redetect(&self) {
        let found = self.port.detect();
        *lock(&self.detected) = Some(found);
    }

    /// What detection found so far; empty until it has run. Never detects.
    fn cached(&self) -> Vec<DetectedTool> {
        lock(&self.detected).clone().unwrap_or_default()
    }

    /// For runs and previews, which already work off the UI thread; may detect.
    pub(super) fn resolve(&self, settings: &GitSettings) -> Option<DetectedTool> {
        tools::choose(settings, &self.detected())
    }

    fn resolve_cached(&self, settings: &GitSettings) -> Option<DetectedTool> {
        tools::choose(settings, &self.cached())
    }

    pub(super) fn can_diff(&self, settings: &GitSettings) -> bool {
        menu::can_diff(self.resolve_cached(settings).as_ref())
    }

    pub(super) fn rows(&self, settings: &GitSettings) -> Vec<ToolRow> {
        let chosen = self.resolve_cached(settings).map(|tool| tool.kind);
        let mut found = self.cached();
        if chosen == Some(ToolKind::Custom) {
            found.extend(tools::choose(settings, &[]));
        }
        found
            .into_iter()
            .map(|tool| ToolRow {
                id: tool.kind.id().to_owned(),
                label: tool.label.to_owned(),
                path: tool.exe.display().to_string(),
                selected: Some(tool.kind) == chosen,
            })
            .collect()
    }

    /// Items for the cached tool, from memory only. A git-cli Pull or Push is pending until
    /// `previews` has read its confirmation.
    pub(super) fn menu(&self, repo: &Location, settings: &GitSettings) -> GitMenu {
        let tool = self.resolve_cached(settings);
        GitMenu {
            head: repo
                .name()
                .and_then(|name| name.as_str().map(str::to_owned))
                .unwrap_or_default(),
            items: menu::items(tool.as_ref()),
        }
    }

    /// Reads the git-cli Pull and Push confirmations and keeps them for `take`, so a run can only
    /// do what the user was shown. Blocks on Git: callers run it off the UI thread.
    pub(super) fn previews(&self, repo: &Location, settings: &GitSettings) -> Previewed {
        let is_cli = self
            .resolve(settings)
            .is_some_and(|tool| tool.kind == ToolKind::GitCli);
        if !is_cli {
            return Vec::new();
        }
        [
            (GitAction::Pull, SyncKind::Pull),
            (GitAction::Push, SyncKind::Push),
        ]
        .into_iter()
        .map(|(action, kind)| (action, self.preview(repo, kind)))
        .collect()
    }

    /// Reads one confirmation and holds it under a fresh id, replacing the one held before.
    fn preview(&self, repo: &Location, kind: SyncKind) -> Option<Shown> {
        let path = navo_fs::os_path(repo).ok()?;
        let outcome = self.sync.confirm(&path, kind);
        let text = outcome.as_ref().ok().map(texts::confirm_text);
        let id = self.previewed.fetch_add(1, Ordering::Relaxed).to_string();
        let mut pending = lock(&self.pending);
        pending.retain(|held| !(held.repo == *repo && held.kind == kind));
        if pending.len() >= PENDING_CAP {
            pending.remove(0);
        }
        pending.push(Pending {
            repo: repo.clone(),
            kind,
            shown: text.clone(),
            id: id.clone(),
            outcome,
        });
        text.map(|text| Shown { text, id })
    }

    /// The confirmation the menu last showed for this Pull or Push, if `confirm_id` is its id
    /// and `shown` its text; it can be used once. An id from an older menu, maybe in another
    /// window, matches nothing and leaves the held one in place.
    pub(super) fn take(
        &self,
        repo: &Location,
        kind: SyncKind,
        (shown, confirm_id): (Option<&str>, Option<&str>),
    ) -> Option<Result<Confirmation, SyncFailure>> {
        let confirm_id = confirm_id?;
        let mut pending = lock(&self.pending);
        let at = pending.iter().position(|held| {
            held.id == confirm_id
                && held.repo == *repo
                && held.kind == kind
                && held.shown.as_deref() == shown
        })?;
        Some(pending.remove(at).outcome)
    }
}
