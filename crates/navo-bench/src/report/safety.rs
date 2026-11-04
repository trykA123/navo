use serde::Serialize;

use super::fmt::{close_table, count, open_table, row};
use super::html;
use super::results::Results;
use crate::manifest::ManifestDiff;
use crate::privacy::{Html, SafeText, SafetyVerdict};

/// The manifest proof that the owner's folder did not change during the run.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum Proof {
    NotMeasured {
        reason: SafeText,
    },
    Equal {
        files: u64,
        folders: u64,
    },
    /// The folder changed, or a side could not be fully walked.
    Fail {
        diff: ManifestDiff,
    },
    /// A check before or after did not finish, so nothing is proved.
    Unfinished,
}

impl Proof {
    pub fn failed(&self) -> bool {
        self.verdict().failed()
    }

    pub fn verdict(&self) -> SafetyVerdict {
        match self {
            Self::NotMeasured { .. } => SafetyVerdict::NotMeasured,
            Self::Equal { .. } => SafetyVerdict::Pass,
            Self::Fail { diff } => SafetyVerdict::Fail {
                entries: diff.changes(),
            },
            Self::Unfinished => SafetyVerdict::Unfinished,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum PhaseOutcome {
    Ok,
    Failed { code: u32 },
    NotMeasured { reason: SafeText },
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct PhaseRow {
    pub name: SafeText,
    pub outcome: PhaseOutcome,
}

/// How the run itself went: the data-safety proof and every phase in order.
#[derive(Clone, Debug, Serialize)]
pub struct RunSummary {
    pub proof: Proof,
    pub phases: Vec<PhaseRow>,
}

fn proof(page: &mut Html, proof: &Proof) {
    match proof {
        Proof::Equal { files, folders } => {
            page.markup("<p><span class=\"badge pass\">Pass</span> Your folder did not change: ")
                .text(&count(*files))
                .markup(" files and ")
                .text(&count(*folders))
                .markup(" folders were checked before and after.</p>");
        }
        Proof::NotMeasured { reason } => {
            page.markup("<p><span class=\"badge\">Not measured</span> <span class=\"muted\">")
                .text(reason.as_str())
                .markup("</span></p>");
        }
        Proof::Unfinished => {
            page.markup("<p><span class=\"badge fail\">FAIL</span> DATA SAFETY: the folder check before or after the run did not finish, so nothing is proved.</p>");
        }
        Proof::Fail { diff } => {
            page.markup("<p><span class=\"badge fail\">FAIL</span> DATA SAFETY: your folder changed during the run, or could not be fully checked.</p>");
            diff_table(page, diff);
        }
    }
}

fn diff_table(page: &mut Html, diff: &ManifestDiff) {
    open_table(
        page,
        "Folder changes",
        &[("Change", false), ("Entries", true)],
    );
    let rows = [
        ("Added", diff.added),
        ("Removed", diff.removed),
        ("Size changed", diff.changed_size),
        ("Modified time changed", diff.changed_mtime),
        ("Attributes changed", diff.changed_attributes),
        ("Content changed", diff.changed_content),
        ("Looks renamed", diff.renamed_looking),
    ];
    for (name, value) in rows {
        row(page, &[(name, false), (&count(value), true)]);
    }
    let walked = |complete: bool| if complete { "complete" } else { "incomplete" };
    row(
        page,
        &[
            ("Check before", false),
            (walked(diff.before_complete), true),
        ],
    );
    row(
        page,
        &[("Check after", false), (walked(diff.after_complete), true)],
    );
    close_table(page);
}

fn phases(page: &mut Html, phases: &[PhaseRow]) {
    page.markup("<h3>Phases</h3>");
    open_table(page, "Phases", &[("Phase", false), ("Result", false)]);
    for phase in phases {
        let result = match phase.outcome {
            PhaseOutcome::Ok => "ok".to_owned(),
            PhaseOutcome::Failed { code } => format!("failed, code {code}"),
            PhaseOutcome::NotMeasured { reason } => format!("not measured: {}", reason.as_str()),
        };
        row(page, &[(phase.name.as_str(), false), (&result, false)]);
    }
    close_table(page);
}

pub(super) fn section(page: &mut Html, results: &Results) {
    html::open_section(page, "safety", "Data safety");
    match results.run.as_ref() {
        Some(run) => {
            proof(page, &run.proof);
            phases(page, &run.phases);
        }
        None => {
            page.markup("<p class=\"muted\">The run did not record its phases.</p>");
        }
    }
    html::close_section(page);
}
