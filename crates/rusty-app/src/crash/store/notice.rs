use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use rusty_core::Location;
use rusty_ui::crash_notice::RecoveryContent;

use super::{
    locks::Locks,
    state::{Notice, State},
};
use rusty_core::brand;

#[derive(Clone)]
pub(crate) struct RecoveryNotice {
    data: PathBuf,
    notices: Vec<Notice>,
    error: Option<String>,
    folder: Option<Location>,
}

impl RecoveryNotice {
    pub(super) fn loaded(data: &Path, state: &State) -> Option<Self> {
        (!state.notices.is_empty()).then(|| Self {
            data: data.to_owned(),
            notices: state.notices.clone(),
            error: None,
            folder: crate::startup::path_location(data),
        })
    }

    pub(super) fn failed(data: &Path, error: &str) -> Self {
        Self {
            data: data.to_owned(),
            notices: Vec::new(),
            error: Some(error.chars().take(512).collect()),
            folder: crate::startup::path_location(data),
        }
    }

    pub fn folder(&self) -> Option<Location> {
        self.folder.clone()
    }

    pub fn content(&self) -> RecoveryContent {
        if let Some(error) = &self.error {
            return RecoveryContent {
                title: rusty_l10n::tr("store-recovery-blocked"),
                details: vec![
                    rusty_l10n::tr("store-recovery-restart"),
                    error.clone(),
                    rusty_l10n::tr("retention-safe-mode-help"),
                ],
            };
        }
        let mut details = Vec::new();
        for notice in self.notices.iter().rev().take(3) {
            details.extend(lines(notice));
        }
        if self.notices.len() > 3 {
            details.push(rusty_l10n::t!(
                "store-recovery-more",
                count = (self.notices.len() - 3) as f64
            ));
        }
        RecoveryContent {
            title: rusty_l10n::tr("store-recovery-title"),
            details,
        }
    }

    pub fn dismiss(&self) -> Result<(), String> {
        if self.notices.is_empty() {
            return Ok(());
        }
        let _startup = Locks::startup(&self.data).map_err(|error| error.to_string())?;
        let mut state = State::load(&self.data).map_err(|error| error.to_string())?;
        state.notices.retain(|notice| {
            !self
                .notices
                .iter()
                .rev()
                .take(3)
                .any(|shown| shown.case == notice.case)
        });
        state.save(&self.data).map_err(|error| error.to_string())
    }
}

fn lines(notice: &Notice) -> Vec<String> {
    let file = format!("{}.db.corrupt-{}", brand::SLUG, notice.slot);
    let key = if notice.database {
        "store-recovery-preserved"
    } else {
        "store-recovery-orphans"
    };
    let mut lines = vec![rusty_l10n::t!(key, file = file)];
    if !notice.database {
        lines.push(rusty_l10n::tr("store-recovery-fresh"));
    }
    if notice.copied_rows > 0 {
        lines.push(rusty_l10n::tr("store-recovery-readable"));
    }
    lines.extend(reset_lines(notice));
    let jobs = if notice.journal_unverified {
        "store-recovery-jobs-unverified"
    } else {
        "store-recovery-jobs"
    };
    lines.push(rusty_l10n::t!(
        jobs,
        restored = notice.recovered_jobs as f64,
        manual = notice.unreadable_jobs as f64
    ));
    lines.extend(notice.pending_paths.iter().take(4).cloned());
    lines.extend(
        notice
            .retry_jobs
            .iter()
            .flat_map(|job| job.paths.iter())
            .take(8)
            .cloned(),
    );
    let file = format!(
        "{}-recovery-{}-{}.jsonl",
        brand::SLUG,
        notice.case,
        notice.attempt
    );
    lines.push(rusty_l10n::t!("store-recovery-details", file = file));
    lines
}

fn reset_lines(notice: &Notice) -> Vec<String> {
    let mut lines = Vec::new();
    let reset = notice
        .reset
        .iter()
        .map(|name| label(name))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if !reset.is_empty() {
        lines.push(rusty_l10n::t!(
            "store-recovery-reset",
            data = reset.join(", ")
        ));
    }
    if notice.reset.iter().any(|name| name == "retained_entry") {
        lines.push(rusty_l10n::tr("store-recovery-retained-manual"));
    }
    lines
}

fn label(name: &str) -> String {
    const LABELS: &[(&str, &str)] = &[
        ("job", "store-recovery-data-jobs"),
        ("job_step", "store-recovery-data-jobs"),
        ("undo", "store-recovery-data-jobs"),
        ("tags", "store-recovery-data-tags"),
        ("module_kv", "store-recovery-data-modules"),
        ("settings_defaults", "store-recovery-data-defaults"),
        ("history", "store-recovery-data-history"),
        ("frecency", "store-recovery-data-recent"),
        ("search_history", "store-recovery-data-search"),
        ("viewer_memory", "store-recovery-data-viewer"),
        ("command_use", "store-recovery-data-commands"),
        ("rename_preset", "store-recovery-data-presets"),
        ("retained_entry", "store-recovery-data-retained"),
    ];
    let key = LABELS
        .iter()
        .find_map(|(table, label)| (*table == name).then_some(*label))
        .unwrap_or("store-recovery-data-other");
    rusty_l10n::tr(key)
}
