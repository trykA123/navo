use super::fields::{Choice, Control, F, Field, FieldSpec, Look, S, choice, spec};

const INDEX_MODES: [Choice; 4] = [
    choice("settings-search-index-auto", "auto"),
    choice("settings-search-index-service", "service"),
    choice("settings-search-index-walk", "walk"),
    choice("settings-search-index-off", "off"),
];
pub(super) const SEARCH_ROWS: [FieldSpec; 6] = [
    spec(
        (F::SearchIndex, S::Search, "search.index"),
        ("settings-search-index", "settings-search-index-help"),
        Control::Choices(&INDEX_MODES, Look::Segments),
    ),
    spec(
        (F::SearchDrives, S::Search, "search.drives"),
        ("settings-search-drives", "settings-search-drives-help"),
        Control::Text,
    ),
    spec(
        (F::SearchExclude, S::Search, "search.exclude"),
        ("settings-search-exclude", "settings-search-exclude-help"),
        Control::Text,
    ),
    spec(
        (F::SearchNetwork, S::Search, "search.network"),
        ("settings-search-network", "settings-search-network-help"),
        Control::Toggle,
    ),
    spec(
        (F::SearchRemovable, S::Search, "search.removable"),
        (
            "settings-search-removable",
            "settings-search-removable-help",
        ),
        Control::Toggle,
    ),
    spec(
        (F::SearchRemember, S::Search, "search.remember"),
        ("settings-search-remember", "settings-search-remember-help"),
        Control::Toggle,
    ),
];

impl Field {
    pub fn is_list(self) -> bool {
        matches!(
            self,
            Self::SearchDrives | Self::SearchExclude | Self::AutomotiveModules
        )
    }

    pub fn is_windows_only(self) -> bool {
        matches!(
            self,
            Self::SearchDrives
                | Self::SearchRemovable
                | Self::ShowSystem
                | Self::KeepInTray
                | Self::SummonKey
        )
    }
}
