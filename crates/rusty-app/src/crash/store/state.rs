use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{Error, files};
use rusty_core::brand;

const LIMIT: u64 = 1_048_576;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    version: u32,
    pub next_case: u64,
    pub pending: Option<Progress>,
    pub notices: Vec<Notice>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            version: 1,
            next_case: 1,
            pending: None,
            notices: Vec::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Identity {
    #[serde(default, skip_serializing_if = "native_kind")]
    pub kind: rusty_core::FileIdentityKind,
    pub volume: u64,
    pub file: [u8; 16],
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Notice {
    pub case: u64,
    pub slot: u64,
    pub attempt: u64,
    pub database: bool,
    pub copied_rows: u64,
    pub reset: Vec<String>,
    pub unreadable_jobs: usize,
    pub journal_unverified: bool,
    pub recovered_jobs: usize,
    pub pending_paths: Vec<String>,
    pub retry_jobs: Vec<RetryJob>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RetryJob {
    pub id: i64,
    pub fingerprint: u64,
    pub paths: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) enum Progress {
    Preserve {
        case: u64,
    },
    Build {
        case: u64,
        slot: u64,
        database: bool,
    },
    Promote {
        notice: Notice,
        identity: Identity,
    },
    Recover {
        notice: Notice,
        identity: Identity,
        attempted: Vec<i64>,
    },
}

impl Progress {
    fn case(&self) -> u64 {
        match self {
            Self::Preserve { case } | Self::Build { case, .. } => *case,
            Self::Promote { notice, .. } | Self::Recover { notice, .. } => notice.case,
        }
    }

    fn valid(&self) -> bool {
        match self {
            Self::Preserve { case } => *case > 0,
            Self::Build { case, slot, .. } => *case > 0 && *slot > 0,
            Self::Promote { notice, .. } | Self::Recover { notice, .. } => notice.valid(),
        }
    }
}

impl Notice {
    fn valid(&self) -> bool {
        self.case > 0 && self.slot > 0 && self.attempt > 0
    }
}

impl State {
    pub fn path(data: &Path) -> PathBuf {
        data.join("store-recovery.json")
    }

    pub fn load(data: &Path) -> Result<Self, Error> {
        let Some(bytes) = files::read(&Self::path(data), LIMIT)? else {
            return Ok(Self::default());
        };
        let state: Self = serde_json::from_slice(&bytes)?;
        state.validate()?;
        Ok(state)
    }

    fn validate(&self) -> Result<(), Error> {
        let mut cases = std::collections::HashSet::new();
        let valid_notices = self.notices.iter().all(|notice| {
            notice.valid() && notice.case < self.next_case && cases.insert(notice.case)
        });
        let valid_pending = self.pending.as_ref().is_none_or(|pending| {
            pending.valid() && pending.case() < self.next_case && !cases.contains(&pending.case())
        });
        if self.version != 1 || self.next_case == 0 || !valid_notices || !valid_pending {
            return Err(Error::Invalid(
                "invalid or unsupported recovery state".into(),
            ));
        }
        Ok(())
    }

    pub fn save(&self, data: &Path) -> Result<(), Error> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() as u64 > LIMIT {
            return Err(Error::Invalid(
                "recovery state exceeds its size limit".into(),
            ));
        }
        files::replace(&Self::path(data), &bytes)?;
        Ok(())
    }

    pub fn begin(&mut self, data: &Path) -> Result<(), Error> {
        let case = self.next_case;
        self.next_case = case
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("recovery case limit reached".into()))?;
        self.pending = Some(Progress::Preserve { case });
        self.save(data)
    }
}

pub(super) fn candidate(data: &Path, case: u64, attempt: u64) -> PathBuf {
    data.join(format!("{}-rebuild-{case}-{attempt}.db", brand::SLUG))
}

pub(super) fn details(data: &Path, case: u64, attempt: u64) -> PathBuf {
    data.join(format!("{}-recovery-{case}-{attempt}.jsonl", brand::SLUG))
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if takes a reference"
)]
fn native_kind(value: &rusty_core::FileIdentityKind) -> bool {
    *value == rusty_core::FileIdentityKind::Native
}
