use super::fields::{Choice, Control, F, Field, FieldSpec, Look, S, choice, spec};

const ENGINES: [Choice; 3] = [
    choice("settings-git-engine-auto", "auto"),
    choice("settings-git-engine-gix", "gix"),
    choice("settings-git-engine-git", "git"),
];

pub(super) const GIT_FIELDS: [FieldSpec; 7] = [
    spec(
        (F::GitBadges, S::Tools, "git.badges"),
        ("settings-git-badges", "settings-git-badges-help"),
        Control::Toggle,
    ),
    spec(
        (F::GitTool, S::Tools, "git.tool"),
        ("settings-git-tool", "settings-git-tool-help"),
        Control::Tool,
    ),
    spec(
        (F::GitToolCommand, S::Tools, "git.tool-command"),
        (
            "settings-git-tool-command",
            "settings-git-tool-command-help",
        ),
        Control::Text,
    ),
    spec(
        (F::GitEngine, S::Tools, "git.engine"),
        ("settings-git-engine", "settings-git-engine-help"),
        Control::Choices(&ENGINES, Look::Segments),
    ),
    spec(
        (F::GitFetch, S::Tools, "git.fetch"),
        ("settings-git-fetch", "settings-git-fetch-help"),
        Control::Toggle,
    ),
    spec(
        (F::GitFetchMinutes, S::Tools, "git.fetch-minutes"),
        (
            "settings-git-fetch-minutes",
            "settings-git-fetch-minutes-help",
        ),
        Control::Number,
    ),
    spec(
        (F::GitNetwork, S::Tools, "git.network"),
        ("settings-git-network", "settings-git-network-help"),
        Control::Toggle,
    ),
];

pub(super) const FETCH_MINUTES: (i64, i64) = (5, 120);

impl Field {
    pub fn number_range(self) -> (i64, i64) {
        match self {
            Self::GitFetchMinutes => FETCH_MINUTES,
            Self::CompareBudgetMib => super::compare_fields::BUDGET_MIB,
            _ => (1, i64::from(u16::MAX)),
        }
    }
}
