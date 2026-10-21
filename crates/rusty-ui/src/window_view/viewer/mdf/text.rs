use rusty_automotive::mdf::ChipKind;
use rusty_automotive::{Mdf, MdfProblem};
use rusty_l10n::t;

use crate::format;

pub(super) fn heads() -> [String; 7] {
    [
        t!("viewer-mdf-head-chip"),
        t!("viewer-mdf-head-type"),
        t!("viewer-mdf-head-size"),
        t!("viewer-mdf-head-space"),
        t!("viewer-mdf-head-start-space"),
        t!("viewer-mdf-head-end"),
        t!("viewer-mdf-head-start-chip"),
    ]
}

const SHOWN_PROBLEMS: usize = 3;

pub(super) struct Check {
    pub text: String,
    pub is_bad: bool,
}

fn hex(value: u64) -> String {
    format!("{value:#010X}")
}

pub(super) fn rows(mdf: &Mdf) -> Vec<Vec<String>> {
    let space_name = |id: u64| {
        mdf.spaces
            .iter()
            .find(|space| space.id == id)
            .map_or_else(|| format!("id {id}"), |space| space.name.clone())
    };
    let mut rows = Vec::new();
    for chip in &mdf.chips {
        let kind = match chip.kind {
            ChipKind::Rom => "ROM",
            ChipKind::Ram => "RAM",
        };
        let head = [chip.name.clone(), kind.to_owned(), format::size(chip.size)];
        let mut mapped = mdf.maps.iter().filter(|map| map.chip == chip.id).peekable();
        if mapped.peek().is_none() {
            rows.push(
                head.iter()
                    .cloned()
                    .chain(["", "", "", ""].map(String::from))
                    .collect(),
            );
        }
        for map in mapped {
            let end = map.start.saturating_add(map.size.saturating_sub(1));
            let tail = [
                space_name(map.space),
                hex(map.start),
                hex(end),
                hex(map.chip_start),
            ];
            rows.push(head.iter().cloned().chain(tail).collect());
        }
    }
    rows
}

pub(super) fn spaces_detail(mdf: &Mdf) -> String {
    if mdf.spaces.is_empty() {
        return t!("viewer-mdf-no-spaces");
    }
    let spaces: Vec<String> = mdf
        .spaces
        .iter()
        .map(|space| format!("{} (id {}, {}-bit)", space.name, space.id, space.mau))
        .collect();
    t!("viewer-mdf-spaces", spaces = spaces.join(", "))
}

fn problem_text(problem: &MdfProblem) -> String {
    match problem {
        MdfProblem::Malformed { line, reason } => {
            t!("viewer-mdf-line", line = *line, reason = *reason)
        }
        MdfProblem::CapReached { line, limit } => {
            t!("viewer-mdf-line-cap", line = *line, limit = *limit)
        }
    }
}

pub(super) fn status(mdf: &Mdf) -> Vec<Check> {
    let mut checks: Vec<Check> = mdf
        .problems
        .iter()
        .take(SHOWN_PROBLEMS)
        .map(|problem| Check {
            text: problem_text(problem),
            is_bad: true,
        })
        .collect();
    let more = mdf.problems.len().saturating_sub(SHOWN_PROBLEMS);
    if more > 0 {
        checks.push(Check {
            text: format!("+{more} more"),
            is_bad: true,
        });
    }
    checks.push(Check {
        text: format!(
            "{} · {}",
            format::count(mdf.chips.len() as u64, "chip", "chips"),
            format::count(mdf.maps.len() as u64, "mapping", "mappings")
        ),
        is_bad: false,
    });
    checks
}
