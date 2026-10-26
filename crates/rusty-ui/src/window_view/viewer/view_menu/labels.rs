use crate::window_view::viewer::parts::ViewParts;

pub(in crate::window_view) const NO_PARTS: &str = "This view has no parts to show or hide.";
pub(in crate::window_view) const KEPT: &str = "Kept as the default in navo.toml.";
pub(in crate::window_view) const SAVING: &str = "Saving\u{2026}";
pub(in crate::window_view) const NOT_KEPT: &str = "Not kept as the default.";
pub(in crate::window_view) const NOT_SAVED: &str = "navo.toml could not be saved.";
pub(in crate::window_view) const MENU_TITLE: &str = "View";
pub(in crate::window_view) const KEEP_LABEL: &str = "Keep as default";
pub(in crate::window_view) const ENCODING: &str = "Encoding\u{2026}";
pub(in crate::window_view) const RESET_LABEL: &str = "Reset to defaults";

pub(in crate::window_view) fn empty_keep_note(parts: &ViewParts) -> &'static str {
    if parts.is_pending() { SAVING } else { KEPT }
}
