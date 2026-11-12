//! The events the compare screen sends to its window instead of calling it directly (NC54).

use crate::settings::Field;

/// What the compare screen asks of its window (NC54).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::window_view) enum ScreenEvent {
    /// The screen closed; reveal and select this path in the compare pane.
    Closed { reveal: Option<String> },
    /// Settings should open with this field selected.
    OpenSettings(Field),
}
