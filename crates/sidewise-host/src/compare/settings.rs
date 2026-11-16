//! Compare settings as plain input.

use navo_compare::model::Budget;
use navo_compare::session::{CheckMode, NetworkCap};
use navo_compare::settings::CompareGitEngine;
use navo_compare::settings::{Compare, ContentCheck};
use navo_fs::{NETWORK_WALK_MAX_ENTRIES, NETWORK_WALK_MAX_TIME};

/// The shared network-walk bound (D103) as the compare engine takes it.
pub fn network_cap() -> NetworkCap {
    NetworkCap {
        entries: NETWORK_WALK_MAX_ENTRIES,
        duration: NETWORK_WALK_MAX_TIME,
    }
}

/// The `[compare]` settings as the compare backend reads them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareSettings {
    pub git_engine: CompareGitEngine,
    pub ignore_list: Vec<String>,
    pub respect_gitignore: bool,
    pub memory_budget_mib: u32,
    pub content_check: CheckMode,
}

impl Default for CompareSettings {
    fn default() -> Self {
        Self {
            git_engine: CompareGitEngine::default(),
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
            git_engine: stored.git_engine,
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
    pub fn budget(&self) -> Budget {
        Budget::from_mib(
            self.memory_budget_mib
                .clamp(Budget::MIN_MIB, Budget::MAX_MIB),
        )
    }
}
