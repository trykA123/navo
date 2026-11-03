use std::sync::Arc;

use navo_core::Location;
use navo_git::SyncKind;
use navo_l10n::tr;
use navo_store::GitSettings;
use navo_ui::git::{GitAction, GitStarted};

use super::hooks::lock;
use super::launch::plan;
use super::texts;
use super::tools::{DetectedTool, ToolKind};
use super::wiring::Tools;

/// Queues a rescan of the repository's listing.
pub(super) type Rescan = Arc<dyn Fn() + Send + Sync>;

pub(super) struct Job<'a> {
    pub repo: &'a Location,
    pub action: GitAction,
    /// The confirmation text the user confirmed, if the menu row had one.
    pub shown: Option<&'a str>,
    /// The id the menu row got with `shown`.
    pub confirm_id: Option<&'a str>,
    pub settings: &'a GitSettings,
    pub rescan: Rescan,
}

impl Tools {
    /// Blocks until a git-cli Pull or Push ends; callers run it off the UI thread. A launched
    /// tool returns at once and queues the rescan when the tool exits.
    pub(super) fn run(&self, job: &Job<'_>) -> Result<GitStarted, String> {
        let tool = self
            .resolve(job.settings)
            .ok_or_else(|| tr("git-fail-no-tool"))?;
        match (tool.kind, job.action) {
            (ToolKind::GitCli, GitAction::Pull) => self.sync(job, SyncKind::Pull, &tool),
            (ToolKind::GitCli, GitAction::Push) => self.sync(job, SyncKind::Push, &tool),
            _ => self.launch(job, &tool),
        }
    }

    fn launch(&self, job: &Job<'_>, tool: &DetectedTool) -> Result<GitStarted, String> {
        let path = os_path(job.repo)?;
        let text = path.to_str().ok_or_else(|| tr("git-fail-bad-path"))?;
        let template = job.settings.tool_command.as_deref();
        let launch = plan(tool, job.action, text, template).map_err(|error| error.to_string())?;
        let rescan = job.rescan.clone();
        let ports = lock(&self.ports).clone();
        self.port
            .start(launch, &ports, Box::new(move || rescan()))
            .map_err(|error| error.to_string())?;
        Ok(GitStarted {
            tool: tool.label.to_owned(),
            result: None,
        })
    }

    fn sync(
        &self,
        job: &Job<'_>,
        kind: SyncKind,
        tool: &DetectedTool,
    ) -> Result<GitStarted, String> {
        let run = self
            .begin_run(job.repo)
            .ok_or_else(|| tr("git-fail-busy"))?;
        let confirmed = self
            .take(job.repo, kind, (job.shown, job.confirm_id))
            .ok_or_else(|| tr("git-fail-reopen"))?
            .map_err(|failure| texts::failure_text(kind, failure))?;
        let path = os_path(job.repo)?;
        let done = self.sync.run(&path, &confirmed, &run.cancel);
        drop(run);
        (job.rescan)();
        let summary = done.map_err(|failure| texts::failure_text(kind, failure))?;
        if summary.tracking_stale {
            tracing::warn!(
                branch = %summary.branch,
                remote = %summary.remote,
                "pull moved HEAD; the remote-tracking ref waits for the next fetch"
            );
        }
        Ok(GitStarted {
            tool: tool.label.to_owned(),
            result: Some(texts::done_text(&summary)),
        })
    }
}

fn os_path(repo: &Location) -> Result<std::path::PathBuf, String> {
    navo_fs::os_path(repo).map_err(|_| tr("git-fail-not-local"))
}
