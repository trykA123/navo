//! The `[compare]` settings: Git engine, ignore rules, memory budget and folder content check.

use serde::{Deserialize, Deserializer, Serialize};

// Memory budget range and default, in MiB.
const MIN_BUDGET_MIB: u32 = 256;
const MAX_BUDGET_MIB: u32 = 16_384;
const DEFAULT_BUDGET_MIB: u32 = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Compare {
    pub git_engine: CompareGitEngine,
    pub ignore_list: Vec<String>,
    pub respect_gitignore: bool,
    #[serde(deserialize_with = "budget_mib")]
    pub memory_budget_mib: u32,
    pub content_check: ContentCheck,
}

impl Default for Compare {
    fn default() -> Self {
        Self {
            git_engine: CompareGitEngine::default(),
            ignore_list: Vec::new(),
            respect_gitignore: true,
            memory_budget_mib: DEFAULT_BUDGET_MIB,
            content_check: ContentCheck::default(),
        }
    }
}

/// Which engine reads Git repositories; a name nobody knows reads as `auto`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CompareGitEngine {
    #[default]
    Git,
    Gix,
    #[serde(other)]
    Auto,
}

/// How a folder compare decides that two files are the same; an unknown name reads as `content`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContentCheck {
    SizeTime,
    #[default]
    #[serde(other)]
    Content,
}

fn budget_mib<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    let mib = i64::deserialize(deserializer)?;
    let clamped = mib.clamp(MIN_BUDGET_MIB.into(), MAX_BUDGET_MIB.into());
    Ok(u32::try_from(clamped).unwrap_or(MAX_BUDGET_MIB))
}
