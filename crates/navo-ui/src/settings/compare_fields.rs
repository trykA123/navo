use super::fields::{Choice, Control, F, FieldSpec, Look, S, choice, spec};

const GIT_ENGINES: [Choice; 3] = [
    choice("settings-compare-git-engine-git", "git"),
    choice("settings-compare-git-engine-gix", "gix"),
    choice("settings-compare-git-engine-auto", "auto"),
];
const CONTENT_CHECKS: [Choice; 2] = [
    choice("settings-compare-content-check-content", "content"),
    choice("settings-compare-content-check-size-time", "size-time"),
];

pub(super) const COMPARE_FIELDS: [FieldSpec; 5] = [
    spec(
        (F::GitEngine, S::Compare, "compare.git-engine"),
        (
            "settings-compare-git-engine",
            "settings-compare-git-engine-help",
        ),
        Control::Choices(&GIT_ENGINES, Look::Segments),
    ),
    spec(
        (F::CompareIgnoreList, S::Compare, "compare.ignore-list"),
        (
            "settings-compare-ignore-list",
            "settings-compare-ignore-list-help",
        ),
        Control::Text,
    ),
    spec(
        (
            F::CompareRespectGitignore,
            S::Compare,
            "compare.respect-gitignore",
        ),
        (
            "settings-compare-respect-gitignore",
            "settings-compare-respect-gitignore-help",
        ),
        Control::Toggle,
    ),
    spec(
        (F::CompareBudgetMib, S::Compare, "compare.memory-budget-mib"),
        (
            "settings-compare-memory-budget",
            "settings-compare-memory-budget-help",
        ),
        Control::Number,
    ),
    spec(
        (F::CompareContentCheck, S::Compare, "compare.content-check"),
        (
            "settings-compare-content-check",
            "settings-compare-content-check-help",
        ),
        Control::Choices(&CONTENT_CHECKS, Look::Segments),
    ),
];
