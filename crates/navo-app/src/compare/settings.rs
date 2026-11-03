//! Compare settings as plain input, and the Git engine choice.

use std::path::Path;

use navo_compare::model::Budget;
use navo_compare::session::CheckMode;
use navo_git::cli::MIN_GIT_VERSION;
use navo_git::{Choice, CliGit, EngineKind, EngineSetting, GitFound, RepoFormat, choose};
use navo_store::{Compare, ContentCheck, GitEngine};

/// The `[compare]` settings as the compare backend reads them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompareSettings {
    pub git_engine: EngineSetting,
    pub ignore_list: Vec<String>,
    pub respect_gitignore: bool,
    pub memory_budget_mib: u32,
    pub content_check: CheckMode,
}

impl Default for CompareSettings {
    fn default() -> Self {
        Self {
            git_engine: EngineSetting::default(),
            ignore_list: Vec::new(),
            respect_gitignore: true,
            memory_budget_mib: Budget::DEFAULT_MIB,
            content_check: CheckMode::Content,
        }
    }
}

impl From<&Compare> for CompareSettings {
    fn from(stored: &Compare) -> Self {
        Self {
            git_engine: match stored.git_engine {
                GitEngine::Git => EngineSetting::Git,
                GitEngine::Gix => EngineSetting::Gix,
                GitEngine::Auto => EngineSetting::Auto,
            },
            ignore_list: stored.ignore_list.clone(),
            respect_gitignore: stored.respect_gitignore,
            memory_budget_mib: stored.memory_budget_mib,
            content_check: match stored.content_check {
                ContentCheck::Content => CheckMode::Content,
                ContentCheck::SizeTime => CheckMode::SizeTime,
            },
        }
    }
}

impl CompareSettings {
    /// The memory budget with the setting clamped to 256 MiB through 16,384 MiB.
    pub(crate) fn budget(&self) -> Budget {
        Budget::from_mib(
            self.memory_budget_mib
                .clamp(Budget::MIN_MIB, Budget::MAX_MIB),
        )
    }
}

/// Picks the Git engine for a repository from the setting, its format and the `git` found.
pub(crate) fn choose_engine(
    settings: &CompareSettings,
    format: &RepoFormat,
    git: GitFound,
) -> Choice {
    choose(settings.git_engine, format, git)
}

/// The engine for one repository. An unreadable format keeps the CLI, whose open reports the error.
pub(crate) fn engine_for(settings: &CompareSettings, repo: &Path) -> EngineKind {
    let Ok(format) = RepoFormat::read(repo) else {
        return EngineKind::Cli;
    };
    choose_engine(settings, &format, git_found(repo)).engine
}

fn git_found(repo: &Path) -> GitFound {
    match CliGit::new(repo) {
        Ok(_) => GitFound::version(MIN_GIT_VERSION.0, MIN_GIT_VERSION.1),
        Err(_) => GitFound::missing(),
    }
}
