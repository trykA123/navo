//! Ignore rules for a compare: the user list plus the root Git ignore files of both sides.

use std::io::Read;

use navo_compare::model::{CaseRule, ReadHint, RelPath, SideSource};
use navo_compare::rules::{IgnoreRules, IgnoreSource, RuleError};
use navo_core::ProviderPath;

use super::settings::CompareSettings;

const MAX_RULE_FILE: u64 = 1024 * 1024;

pub(super) fn build(
    left: &dyn SideSource,
    right: &dyn SideSource,
    settings: &CompareSettings,
) -> Result<IgnoreRules, RuleError> {
    let mut sources = Vec::new();
    if settings.respect_gitignore {
        for side in [left, right] {
            if let Some(contents) = read_text(side, ".gitignore") {
                sources.push(IgnoreSource::Gitignore {
                    folder: RelPath::root(),
                    contents,
                });
            }
            if let Some(contents) = read_text(side, ".git/info/exclude") {
                sources.push(IgnoreSource::Exclude { contents });
            }
        }
    }
    let case = CaseRule::for_sides(left.case_sensitive(), right.case_sensitive());
    let (rules, problems) = IgnoreRules::new(sources, &settings.ignore_list, false, case)?;
    for problem in problems {
        tracing::debug!(source = %problem.source, line = problem.line, "ignore rule skipped");
    }
    Ok(rules)
}

fn read_text(side: &dyn SideSource, name: &str) -> Option<String> {
    let rel = RelPath::new(ProviderPath::from_text(name));
    let reader = side.open(&rel, ReadHint::Buffered).ok()?;
    let mut bytes = Vec::new();
    reader.take(MAX_RULE_FILE).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}
