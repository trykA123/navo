use crate::cli::MIN_GIT_VERSION;
use crate::format::{ObjectFormat, RefStorage, RepoFormat};

pub const CLI_ENGINE_ID: &str = "git";
pub const GIX_ENGINE_ID: &str = "gix";

pub const NO_GIT_NOTE: &str = "compare-git-no-git";
pub const INSTALL_GIT_NOTE: &str = "compare-git-install-git";
pub const UNSUPPORTED_FORMAT_NOTE: &str = "compare-git-unsupported-format";
pub const MERGE_BASE_GIX_NOTE: &str = "compare-git-merge-base-gix";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    Cli,
    Gix,
}

impl EngineKind {
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Cli => CLI_ENGINE_ID,
            Self::Gix => GIX_ENGINE_ID,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EngineSetting {
    #[default]
    Git,
    Gix,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitFound {
    Missing,
    Version { major: u32, minor: u32 },
}

impl GitFound {
    #[must_use]
    pub const fn missing() -> Self {
        Self::Missing
    }

    #[must_use]
    pub const fn version(major: u32, minor: u32) -> Self {
        Self::Version { major, minor }
    }

    #[must_use]
    pub const fn is_usable(self) -> bool {
        match self {
            Self::Missing => false,
            Self::Version { major, minor } => {
                major > MIN_GIT_VERSION.0
                    || (major == MIN_GIT_VERSION.0 && minor >= MIN_GIT_VERSION.1)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice {
    pub engine: EngineKind,
    pub note: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Work {
    Read,
    Filters,
    WorktreeStatus,
    MergeBase,
}

impl Choice {
    #[must_use]
    pub fn for_work(self, work: Work, git: GitFound) -> Self {
        match (work, git.is_usable()) {
            (Work::Read, _) => self,
            (_, true) => Self {
                engine: EngineKind::Cli,
                note: None,
            },
            (Work::MergeBase, false) => Self {
                engine: self.engine,
                note: Some(MERGE_BASE_GIX_NOTE),
            },
            (_, false) => Self {
                engine: self.engine,
                note: Some(INSTALL_GIT_NOTE),
            },
        }
    }
}

#[must_use]
pub fn choose(setting: EngineSetting, format: &RepoFormat, git: GitFound) -> Choice {
    let gix_reads = supports_gix(format);
    match setting {
        EngineSetting::Git if git.is_usable() => cli(None),
        EngineSetting::Git if gix_reads => gix(Some(NO_GIT_NOTE)),
        EngineSetting::Git => gix(Some(INSTALL_GIT_NOTE)),
        EngineSetting::Auto if gix_reads => gix(None),
        EngineSetting::Auto if git.is_usable() => cli(None),
        EngineSetting::Auto => gix(Some(INSTALL_GIT_NOTE)),
        EngineSetting::Gix if gix_reads => gix(None),
        EngineSetting::Gix if git.is_usable() => cli(Some(UNSUPPORTED_FORMAT_NOTE)),
        EngineSetting::Gix => gix(Some(INSTALL_GIT_NOTE)),
    }
}

fn supports_gix(format: &RepoFormat) -> bool {
    format.object_format == ObjectFormat::Sha1 && format.ref_storage == RefStorage::Files
}

const fn cli(note: Option<&'static str>) -> Choice {
    Choice {
        engine: EngineKind::Cli,
        note,
    }
}

const fn gix(note: Option<&'static str>) -> Choice {
    Choice {
        engine: EngineKind::Gix,
        note,
    }
}
