//! The compare screen's key file and label table, shared by NV and Sidewise (NC114).
//! Parsing here needs no command registry.

/// The compare screen's key file, which both apps load.
pub const COMPARE_KEYMAP: &str = include_str!("commands/compare_keymap.toml");

/// One binding from the compare key file.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct CompareKey {
    pub command: String,
    pub keys: Vec<String>,
    pub context: String,
}

#[derive(serde::Deserialize)]
struct File {
    bind: Vec<CompareKey>,
}

/// Every binding in the key file, in file order; fails with the parser's message.
pub fn compare_keys() -> Result<Vec<CompareKey>, String> {
    toml::from_str::<File>(COMPARE_KEYMAP)
        .map(|file| file.bind)
        .map_err(|error| error.to_string())
}

/// Command name → Fluent id of its label, for every command the compare key file binds.
pub const COMPARE_LABELS: &[(&str, &str)] = &[
    ("compare.with_other_pane", "cmd-compare-with-other-pane"),
    ("compare.refs", "cmd-compare-refs"),
    ("compare.next_difference", "cmd-compare-next-difference"),
    (
        "compare.previous_difference",
        "cmd-compare-previous-difference",
    ),
    ("compare.enter", "cmd-compare-enter"),
    ("compare.up", "cmd-compare-up"),
    ("compare.exit", "cmd-compare-exit"),
    ("compare.stop_checking", "cmd-compare-stop-checking"),
    ("compare.continue", "cmd-compare-continue"),
    ("compare.stream", "cmd-compare-stream"),
    ("compare.copy_right", "cmd-compare-copy-right"),
    ("compare.copy_left", "cmd-compare-copy-left"),
    ("compare.save", "cmd-compare-save"),
    ("compare.view_bytes", "cmd-compare-view-bytes"),
    ("compare.view_text", "cmd-compare-view-text"),
    ("compare.show_bytes_here", "cmd-compare-show-bytes-here"),
    ("compare.next_file", "cmd-compare-next-file"),
    ("compare.previous_file", "cmd-compare-previous-file"),
    ("compare.view_menu", "cmd-compare-view-menu"),
    ("compare.changes", "cmd-compare-changes"),
    ("compare.details", "cmd-compare-details"),
    ("compare.pick_down", "cmd-compare-pick-down"),
    ("compare.pick_up", "cmd-compare-pick-up"),
    ("compare.pick_toggle", "cmd-compare-pick-toggle"),
    ("compare.remove_picked", "cmd-compare-remove-picked"),
    ("compare.show_menu", "cmd-compare-show-menu"),
    ("compare.toggle_wrap", "cmd-compare-toggle-wrap"),
    ("compare.toggle_viewed", "cmd-compare-toggle-viewed"),
    ("compare.fold", "cmd-compare-fold"),
    ("compare.open_file", "cmd-compare-open-file"),
    ("compare.search", "cmd-compare-search"),
    ("compare.copy_all_left", "cmd-compare-copy-all-left"),
    ("compare.copy_all_right", "cmd-compare-copy-all-right"),
    // Commands from other groups that the compare keys also bind.
    ("files.undo", "cmd-files-undo"),
    ("find.go_to_result", "cmd-find-go-to-result"),
    ("find.result_down", "cmd-find-result-down"),
    ("find.result_up", "cmd-find-result-up"),
    ("viewer.go_to_end", "cmd-viewer-go-to-end"),
    ("viewer.go_to_start", "cmd-viewer-go-to-start"),
    ("viewer.line_down", "cmd-viewer-line-down"),
    ("viewer.line_up", "cmd-viewer-line-up"),
    ("viewer.page_down", "cmd-viewer-page-down"),
    ("viewer.page_up", "cmd-viewer-page-up"),
    ("viewer.scroll_left", "cmd-viewer-scroll-left"),
    ("viewer.scroll_right", "cmd-viewer-scroll-right"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_compare_binding_has_a_label() {
        let keys = compare_keys().expect("the compare key file parses");
        assert!(!keys.is_empty());
        for key in keys {
            assert!(
                COMPARE_LABELS.iter().any(|(name, _)| *name == key.command),
                "no label for {}",
                key.command
            );
        }
    }
}
